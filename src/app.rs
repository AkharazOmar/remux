use anyhow::Result;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use prost::Message;
use crate::video::device_monitor::{
    DeviceMonitor,
    video_device::VideoDeviceList,
    video_device::StreamControl
};
use crate::video::streamer::Streamer;
use crate::com::service::{Service};
use std::collections::HashMap;

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
    streamers: HashMap<String, Streamer>,
    service: Service,
}

impl App {
    /// Create a new application instance
    pub async fn new() -> Result<Self> {
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
        Ok(Self {
            event_tx,
            event_rx,
            device_monitor,
            streamers: HashMap::new(),
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
            self.streamers.insert(device.device_path.clone(), Streamer::new(&device.device_path).unwrap());
            self.streamers.get_mut(&device.device_path).unwrap().start().await?;
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
        if let Some(streamer) = self.streamers.get_mut(&message.device_path) {
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

    #[tokio::test]
    async fn test_app_creation() {
        let app = App::new().await;
        assert!(app.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_stream_control_subscription() {
        let app = App::new().await;
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
}
