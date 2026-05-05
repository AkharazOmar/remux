use anyhow::Result;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use prost::Message;
use crate::video::v4l2::pipeline::V4L2Pipeline;
use crate::video::rtsp::pipeline::RtspPipeline;
use crate::video::video_device::{
    VideoDeviceList,
    StreamControl
};
use crate::video::v4l2::device_monitor::DeviceMonitor;
use crate::video::streamer::Streamer;
use crate::com::service::Service;
use std::collections::HashMap;

use crate::config::Config;


/// Application events
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Device list changed
    DevicesChanged(VideoDeviceList),
    /// Scan for devices
    ScanDevices,
    /// Handle stream control message
    StreamControlMessage(StreamControl),
}

/// Main application structure
pub struct App {
    event_tx: mpsc::Sender<AppEvent>,
    event_rx: mpsc::Receiver<AppEvent>,
    device_monitor: DeviceMonitor,
    v4l2_streamers: HashMap<String, Streamer>,
    rtsp_streamers: HashMap<String, Streamer>,
    service: Service,
}

impl App {
    /// Create a new application instance
    pub async fn new(config_path: Option<&str>) -> Result<Self> {
        let config = match config_path {
            Some(path) => Config::load_from_file(std::path::Path::new(path))?,
            None => Config::default(),
        };
        let (event_tx, event_rx) = mpsc::channel(100);
        let device_monitor = DeviceMonitor::new()?;

        // Create Zenoh session
        let service = Service::new().await.unwrap();
        let stream_control_subscriber = service.stream_control_subscriber.clone();
        let event_tx_clone = event_tx.clone();
        tokio::spawn(async move {
            while let Ok(sample) = stream_control_subscriber.recv_async().await {
                let payload = sample.payload().to_bytes();
                let stream_control = match StreamControl::decode(payload.as_ref()) {
                    Ok(message) => message,
                    Err(err) => {
                        println!("Failed to decode StreamControl message: {}", err);
                        continue;
                    }
                };

                println!(
                    "Stream Control Message: device_path={}, start={}",
                    stream_control.device_path,
                    stream_control.start
                );
                let _ = event_tx_clone
                    .send(AppEvent::StreamControlMessage(stream_control))
                    .await;
            }
        });
        let mut rtsp_streamers = HashMap::new();
        for camera in &config.rtsp {
            eprintln!("Configuring RTSP camera: name={}, uri={}, protocol={}", camera.name, camera.uri, camera.protocol);
            let pipeline = RtspPipeline { name: camera.name.clone(), url: camera.uri.clone(), protocol: camera.protocol.clone()};
            rtsp_streamers.insert(camera.name.clone(), Streamer::new(pipeline)?);
        }
        Ok(Self {
            event_tx,
            event_rx,
            device_monitor,
            v4l2_streamers: HashMap::new(),
            rtsp_streamers,
            service,
        })
    }

    /// Get a sender for sending events to the application
    pub fn event_sender(&self) -> mpsc::Sender<AppEvent> {
        self.event_tx.clone()
    }

    /// Run the application event loop
    pub async fn run(&mut self) -> Result<()> {
        println!("Starting Remux application...");

        // Start RTSP streamers
        for (name, streamer) in &self.rtsp_streamers {
            eprintln!("Starting RTSP streamer: {}", name);
            streamer.start().await?;
        }

        // Initial device scan
        self.scan_devices().await?;

        // Setup periodic device scanning (every 5 seconds)
        let event_tx = self.event_sender();
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(5));
            loop {
                ticker.tick().await;
                if event_tx.send(AppEvent::ScanDevices).await.is_err() {
                    break;
                }
            }
        });

        // Main event loop
        while let Some(event) = self.event_rx.recv().await {
            match event {
                AppEvent::ScanDevices => {
                    self.scan_devices().await?;
                }
                AppEvent::DevicesChanged(device_list) => {
                    self.handle_devices_changed(device_list).await?;
                }
                AppEvent::StreamControlMessage(message) => {
                    self.stream_control_handler(message).await?;
                }
            }
        }

        Ok(())
    }

    /// Scan for video devices
    async fn scan_devices(&self) -> Result<()> {
        if let Some(device_list) = self.device_monitor.scan_devices().await? {
            self.event_tx.send(AppEvent::DevicesChanged(device_list)).await
                .map_err(|_| anyhow::anyhow!("Failed to send DevicesChanged event"))?;
        }
        Ok(())
    }

    /// Handle device list changes
    async fn handle_devices_changed(& mut self, device_list: VideoDeviceList) -> Result<()> {
        println!("\n=== Video Devices Update ===");
        println!("Found {} device(s)\n", device_list.devices.len());

        for (idx, device) in device_list.devices.iter().enumerate() {
            println!("Device #{}", idx + 1);
            println!("  Name: {}", device.name);
            println!("  Path: {}", device.device_path);
            println!("  Class: {}", device.device_class);
            println!("  Formats: {} available", device.formats.len());
            println!();
            self.v4l2_streamers.insert(device.device_path.clone(), Streamer::new(V4L2Pipeline{device_path: device.device_path.clone()}).unwrap());
            self.v4l2_streamers.get_mut(&device.device_path).unwrap().start().await?;
        }

        // Publish to Zenoh
        let mut buf = vec![];
        device_list.encode(&mut buf)?;
        self.service.video_devices_put(buf).await
            .map_err(|e| anyhow::anyhow!("Failed to publish to Zenoh: {}", e))?;

        Ok(())
    }
    /// Handle stream control messages
    async fn stream_control_handler(&mut self, message: StreamControl) -> Result<()> {
        println!(
            "Received stream control message: device_path={}, start={}",
            message.device_path,
            message.start
        );
        if let Some(streamer) = self.v4l2_streamers.get_mut(&message.device_path) {
            if message.start {
                let format = message.format.unwrap();
                streamer.update_caps(format.format.as_str(), format.width as u32, format.height as u32).await?;
                if streamer.get_state() != crate::video::streamer::StreamerState::Starting {
                    streamer.start().await?;
                }
            } else {
                if streamer.get_state() == crate::video::streamer::StreamerState::Starting {
                    streamer.pause().await?;
                }
            }
        } else {
            println!("No streamer found for device path: {}", message.device_path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gstreamer::prelude::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_app_creation_without_config() {
        let app = App::new(None).await;
        assert!(app.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_app_creation_with_valid_config() {
        use std::io::Write;
        let mut file = tempfile::NamedTempFile::new().unwrap();
        write!(file, r#"
[[rtsp]]
name = "Test Camera"
uri = "rtsp://localhost/stream"
protocol = "tcp"
"#).unwrap();

        let app = App::new(Some(file.path().to_str().unwrap())).await;
        assert!(app.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_app_creation_with_invalid_config() {
        let result = App::new(Some("/nonexistent/config.toml")).await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_stream_control_subscription() {
        let app = App::new(None).await;
        assert!(app.is_ok());
        let mut app = app.unwrap();
        let session = app.service.session.clone();
        let publisher = session
            .declare_publisher("video/stream_control")
            .await
            .unwrap();
        publisher
            .put("Test Stream Control Message 1".as_bytes().to_vec())
            .await
            .unwrap();

        let receive_result = tokio::time::timeout(Duration::from_millis(200), app.event_rx.recv()).await;
        assert!(receive_result.is_err(), "invalid stream control message should be ignored");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_event_sender() {
        let app = App::new(None).await.unwrap();
        let sender = app.event_sender();
        let result = sender.send(AppEvent::ScanDevices).await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_handle_devices_changed_empty_list() {
        let mut app = App::new(None).await.unwrap();
        let device_list = VideoDeviceList { devices: vec![] };
        let result = app.handle_devices_changed(device_list).await;
        assert!(result.is_ok());
        assert!(app.v4l2_streamers.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_stream_control_handler_unknown_device() {
        let mut app = App::new(None).await.unwrap();
        let message = StreamControl {
            device_path: "/dev/video99".to_string(),
            start: true,
            format: None,
        };
        // Should not error, just print "No streamer found"
        let result = app.stream_control_handler(message).await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_stream_control_handler_stop_unknown_device() {
        let mut app = App::new(None).await.unwrap();
        let message = StreamControl {
            device_path: "/dev/video99".to_string(),
            start: false,
            format: None,
        };
        let result = app.stream_control_handler(message).await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_scan_devices() {
        let app = App::new(None).await.unwrap();
        let result = app.scan_devices().await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_app_creation_with_multiple_rtsp() {
        use std::io::Write;
        let mut file = tempfile::NamedTempFile::new().unwrap();
        write!(file, r#"
[[rtsp]]
name = "Camera 1"
uri = "rtsp://localhost/stream1"
protocol = "tcp"

[[rtsp]]
name = "Camera 2"
uri = "rtsp://localhost/stream2"
"#).unwrap();

        let app = App::new(Some(file.path().to_str().unwrap())).await;
        assert!(app.is_ok());
        let app = app.unwrap();
        assert_eq!(app.rtsp_streamers.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_stream_control_start_with_existing_v4l2_streamer() {
        use crate::video::streamer::PipelineFactory;
        use crate::video::video_device::VideoFormat;

        struct FakePipeline;
        impl PipelineFactory for FakePipeline {
            fn name(&self) -> &str { "fake" }
            fn create_pipeline(&self) -> anyhow::Result<gstreamer::Pipeline> {
                gstreamer::init().unwrap();
                let pipeline = gstreamer::Pipeline::with_name("fake-v4l2");
                let src = gstreamer::ElementFactory::make("videotestsrc")
                    .name("source")
                    .build().unwrap();
                let capsfilter = gstreamer::ElementFactory::make("capsfilter")
                    .name(crate::video::streamer::CAPSFILTER)
                    .build().unwrap();
                let sink = gstreamer::ElementFactory::make("fakesink").build().unwrap();
                pipeline.add_many([&src, &capsfilter, &sink]).unwrap();
                gstreamer::Element::link_many([&src, &capsfilter, &sink]).unwrap();
                Ok(pipeline)
            }
        }

        let mut app = App::new(None).await.unwrap();
        let device_path = "/dev/video_fake".to_string();
        app.v4l2_streamers.insert(device_path.clone(), Streamer::new(FakePipeline).unwrap());

        // Start with format
        let message = StreamControl {
            device_path: device_path.clone(),
            start: true,
            format: Some(VideoFormat {
                mime_type: String::new(),
                format: "video/x-raw".to_string(),
                width: 640,
                height: 480,
                framerate_num: 30,
                framerate_den: 1,
            }),
        };
        let result = app.stream_control_handler(message).await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_stream_control_stop_with_existing_v4l2_streamer() {
        use crate::video::streamer::PipelineFactory;

        struct FakePipeline;
        impl PipelineFactory for FakePipeline {
            fn name(&self) -> &str { "fake" }
            fn create_pipeline(&self) -> anyhow::Result<gstreamer::Pipeline> {
                gstreamer::init().unwrap();
                let pipeline = gstreamer::Pipeline::with_name("fake-v4l2-stop");
                let src = gstreamer::ElementFactory::make("videotestsrc").build().unwrap();
                let sink = gstreamer::ElementFactory::make("fakesink").build().unwrap();
                pipeline.add_many([&src, &sink]).unwrap();
                gstreamer::Element::link_many([&src, &sink]).unwrap();
                Ok(pipeline)
            }
        }

        let mut app = App::new(None).await.unwrap();
        let device_path = "/dev/video_fake_stop".to_string();
        app.v4l2_streamers.insert(device_path.clone(), Streamer::new(FakePipeline).unwrap());

        // Start first so we have a Starting state
        let start_msg = StreamControl {
            device_path: device_path.clone(),
            start: true,
            format: Some(crate::video::video_device::VideoFormat {
                mime_type: String::new(),
                format: "video/x-raw".to_string(),
                width: 640,
                height: 480,
                framerate_num: 30,
                framerate_den: 1,
            }),
        };
        app.stream_control_handler(start_msg).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Stop
        let stop_msg = StreamControl {
            device_path: device_path.clone(),
            start: false,
            format: None,
        };
        let result = app.stream_control_handler(stop_msg).await;
        assert!(result.is_ok());
    }
}
