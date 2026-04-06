use anyhow::{anyhow, Result};
use gstreamer as gst;
use gst::prelude::*;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;



/// Streamer states
#[derive(Debug, Clone, PartialEq)]
pub enum StreamerState {
    Stopped,
    Starting,
    Playing,
    Paused,
    Error(String),
}


/// Commands sent to the GStreamer thread
#[derive(Debug)]
enum GstCommand {
    Start,
    Stop,
    Pause,
    Shutdown,
    UpdateCaps(gst::Caps),
}

/// Video streamer using GStreamer
pub struct Streamer {
    command_tx: mpsc::Sender<GstCommand>,
    state: Arc<Mutex<StreamerState>>,
}

pub const CAPSFILTER: &str = "capsfilter";

pub trait PipelineFactory: Send + 'static {
    fn name(&self) -> &str;
    fn create_pipeline(&self) -> Result<gst::Pipeline>;
}

impl Streamer {
    /// Create a new streamer for a V4L2 device
    pub fn new(factory: impl PipelineFactory) -> Result<Self> {
        let state = Arc::new(Mutex::new(StreamerState::Stopped));
        let (command_tx, command_rx) = mpsc::channel::<GstCommand>(32);

        // Clone for the GStreamer thread
        let state_clone = Arc::clone(&state);

        // Spawn dedicated GStreamer thread
        std::thread::spawn(move || {
            Self::gstreamer_thread(factory, command_rx, state_clone);
        });

        Ok(Self {
            command_tx,
            state,
        })
    }

    /// GStreamer thread - handles all GStreamer operations
    fn gstreamer_thread(
        factory: impl PipelineFactory,
        command_rx: mpsc::Receiver<GstCommand>,
        state: Arc<Mutex<StreamerState>>,
    ) {
        // Initialize GStreamer in this thread
        if let Err(e) = gst::init() {
            eprintln!("Failed to initialize GStreamer: {}", e);
            return;
        }

        // Create pipeline
        let pipeline = match factory.create_pipeline() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to create pipeline: {}", e);
                return;
            }
        };

        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

        // Set up bus watch (thread-safe version)
        let _bus_watch = if let Some(bus) = pipeline.bus() {
            let state_clone = Arc::clone(&state);
            let running_clone = Arc::clone(&running);

            Some(bus.add_watch(move |_, msg| {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Eos(..) => {
                        eprintln!("End-Of-Stream reached");
                        running_clone.store(false, std::sync::atomic::Ordering::SeqCst);
                        glib::ControlFlow::Break
                    }
                    MessageView::Error(err) => {
                        let error_msg = format!(
                            "Error from {:?}: {} ({:?})",
                            err.src().map(|s| s.path_string()),
                            err.error(),
                            err.debug()
                        );
                        eprintln!("GStreamer error: {}", error_msg);
                        {
                            let mut state_guard = state_clone.lock().unwrap();
                            *state_guard = StreamerState::Error(error_msg);
                        }
                        running_clone.store(false, std::sync::atomic::Ordering::SeqCst);
                        glib::ControlFlow::Break
                    }
                    MessageView::Warning(warning) => {
                        eprintln!(
                            "Warning from {:?}: {} ({:?})",
                            warning.src().map(|s| s.path_string()),
                            warning.error(),
                            warning.debug()
                        );
                        glib::ControlFlow::Continue
                    }
                    _ => glib::ControlFlow::Continue,
                }
            })
            .expect("Failed to add bus watch"))
        } else {
            None
        };

        // Command loop — poll for commands while running
        let mut command_rx = command_rx;
        while running.load(std::sync::atomic::Ordering::SeqCst) {
            match command_rx.try_recv() {
                Ok(cmd) => {
                    Self::handle_command(&cmd, &pipeline, &state, &running);
                }
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }

        // Cleanup
        let _ = pipeline.set_state(gst::State::Null);
    }

    /// Handle a command from the command channel
    fn handle_command(
        cmd: &GstCommand,
        pipeline: &gst::Pipeline,
        state: &Arc<Mutex<StreamerState>>,
        running: &Arc<std::sync::atomic::AtomicBool>,
    ) {
        match cmd {
            GstCommand::Start => {
                {
                    let mut state_guard = state.lock().unwrap();
                    *state_guard = StreamerState::Starting;
                }

                match pipeline.set_state(gst::State::Playing) {
                    Ok(_) => {
                        {
                            let mut state_guard = state.lock().unwrap();
                            *state_guard = StreamerState::Playing;
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to start pipeline: {}", e);
                        eprintln!("{}", error_msg);
                        {
                            let mut state_guard = state.lock().unwrap();
                            *state_guard = StreamerState::Error(error_msg.clone());
                        }
                    }
                }
            }
            GstCommand::Stop => {
                eprintln!("Handling Stop command, setting state to Null...");
                match pipeline.set_state(gst::State::Null) {
                    Ok(_) => {
                        eprintln!("Pipeline state set to Null successfully");
                        {
                            let mut state_guard = state.lock().unwrap();
                            *state_guard = StreamerState::Stopped;
                        }
                        eprintln!("Sent StateChanged(Stopped) event");
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to stop pipeline: {}", e);
                        eprintln!("{}", error_msg);
                    }
                }
            }
            GstCommand::Pause => {
                match pipeline.set_state(gst::State::Paused) {
                    Ok(_) => {
                        {
                            let mut state_guard = state.lock().unwrap();
                            *state_guard = StreamerState::Paused;
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to pause pipeline: {}", e);
                        eprintln!("{}", error_msg);
                    }
                }
            }
            GstCommand::UpdateCaps(caps) => {
                let capsfilter = match pipeline
                    .by_name(CAPSFILTER)
                    .and_then(|e| e.dynamic_cast::<gst::Element>().ok())
                {
                    Some(elem) => elem,
                    None => {
                        eprintln!("Capsfilter element not found in pipeline");
                        return;
                    }
                };

                let caps_supported = pipeline
                    .by_name("source")
                    .and_then(|e| e.static_pad("src"))
                    .map(|pad| pad.query_caps(None).can_intersect(&caps))
                    .unwrap_or(true);

                if !caps_supported {
                    eprintln!("Requested caps are not supported by the source, skipping update");
                    return;
                }

                capsfilter.set_property("caps", caps.clone());
                eprintln!("Updated caps on capsfilter");

                let _ = pipeline.send_event(gst::event::Reconfigure::new());
            }
            GstCommand::Shutdown => {
                let _ = pipeline.set_state(gst::State::Null);
                running.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }

    /// Handle a GStreamer bus message
    fn handle_bus_message(
        msg: &gst::Message,
        state: &Arc<Mutex<StreamerState>>,
        main_loop: &glib::MainLoop,
    ) -> glib::ControlFlow {
        use gst::MessageView;

        match msg.view() {
            MessageView::Eos(..) => {
                eprintln!("End-Of-Stream reached");
                main_loop.quit();
                glib::ControlFlow::Break
            }
            MessageView::Error(err) => {
                let error_msg = format!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                eprintln!("GStreamer error: {}", error_msg);

                {
                    let mut state_guard = state.lock().unwrap();
                    *state_guard = StreamerState::Error(error_msg.clone());
                }

                main_loop.quit();
                glib::ControlFlow::Break
            }
            MessageView::StateChanged(state_changed) => {
                if let Some(element) = msg.src() {
                    let old_state = state_changed.old();
                    let new_state = state_changed.current();
                    eprintln!(
                        "State changed from {:?} to {:?} for element {}",
                        old_state,
                        new_state,
                        element.name()
                    );
                    // Highlight transitions to Null
                    if new_state == gst::State::Null {
                        eprintln!("  ^^^ TRANSITION TO NULL ^^^");
                    }
                }
                glib::ControlFlow::Continue
            }
            MessageView::Warning(warning) => {
                eprintln!(
                    "Warning from {:?}: {} ({:?})",
                    warning.src().map(|s| s.path_string()),
                    warning.error(),
                    warning.debug()
                );
                glib::ControlFlow::Continue
            }
            _ => glib::ControlFlow::Continue,
        }
    }

    /// Start streaming
    pub async fn start(&self) -> Result<()> {
        self.command_tx
            .send(GstCommand::Start)
            .await
            .map_err(|e| anyhow!("Failed to send start command: {}", e))
    }

    /// Stop streaming
    pub async fn stop(&self) -> Result<()> {
        self.command_tx
            .send(GstCommand::Stop)
            .await
            .map_err(|e| anyhow!("Failed to send stop command: {}", e))
    }

    /// Pause streaming
    pub async fn pause(&self) -> Result<()> {
        self.command_tx
            .send(GstCommand::Pause)
            .await
            .map_err(|e| anyhow!("Failed to send pause command: {}", e))
    }

    pub async fn update_caps(&self, format: &str, width: u32, height: u32) -> Result<()> {
        let mut caps_builder = gst::Caps::builder(format)
            .field("width", width)
            .field("height", height);

        if format == "video/x-raw" {
            caps_builder = caps_builder
                .field("format", "YUY2")
                .field("framerate", gst::Fraction::new(30, 1));
        }

        let caps = caps_builder.build();

        self.command_tx
            .send(GstCommand::UpdateCaps(caps))
            .await
            .map_err(|e| anyhow!("Failed to send update caps command: {}", e))
    }

    /// Get current state
    pub fn get_state(&self) -> StreamerState {
        self.state.lock().unwrap().clone()
    }
}

pub fn create_decode_sink_chain(pipeline: &gst::Pipeline, _title: &str) -> Result<gst::Element> {
    let decobin = gst::ElementFactory::make("decodebin")
        .build()
        .map_err(|e| anyhow!("Failed to create decodebin: {}", e))?;

    let videoconvert = gst::ElementFactory::make("videoconvert")
        .build()
        .map_err(|e| anyhow!("Failed to create videoconvert: {}", e))?;

    let sink = gst::ElementFactory::make("autovideosink")
        .property_from_str("name", _title)
        .build()
        .map_err(|e| anyhow!("Failed to create autovideosink: {}", e))?;

    pipeline.add_many([&decobin, &videoconvert, &sink])?;
    gst::Element::link_many([&videoconvert, &sink])?;

    let videoconvert_weak = videoconvert.downgrade();
    decobin.connect_pad_added(move |_, src_pad| {
        let Some(videoconvert) = videoconvert_weak.upgrade() else { return; };
        let sink_pad = videoconvert.static_pad("sink").expect("Failed to get sink pad from videoconvert");
        if sink_pad.is_linked() {
            eprintln!("Sink pad already linked, ignoring");
            return;
        }

        let src_pad_caps = src_pad.current_caps().or_else(|| Some(src_pad.query_caps(None)));
        let Some(src_pad_caps) = src_pad_caps else {
            eprintln!("Failed to get caps from src pad");
            return;
        };
        let Some(src_pad_struct) = src_pad_caps.structure(0) else {return;};
        let src_pad_type = src_pad_struct.name();

        if src_pad_type.starts_with("video/") {
            match src_pad.link(&sink_pad) {
                Ok(_) => eprintln!("Linked decodebin src pad to videoconvert sink pad"),
                Err(e) => eprintln!("Failed to link decodebin src pad to videoconvert sink pad: {}", e),
            }
        }
    });

    Ok(decobin)
}

impl Drop for Streamer {
    fn drop(&mut self) {
        // Send shutdown command to GStreamer thread
        // Use try_send to avoid blocking in async context
        let _ = self.command_tx.try_send(GstCommand::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_streamer_creation() {
        let streamer = Streamer::new("/dev/video0");
        assert!(streamer.is_ok());
    }

    #[tokio::test]
    async fn test_streamer_state() {
        let streamer = Streamer::new("/dev/video0").unwrap();
        assert_eq!(streamer.get_state(), StreamerState::Stopped);
    }

    #[tokio::test]
    async fn test_streamer_start_stop() {
        let streamer = Streamer::new("/dev/video0").unwrap();

        eprintln!("Starting streamer...");
        let start_result = streamer.start().await;
        assert!(start_result.is_ok());

        sleep(Duration::from_secs(5)).await;
        let update_caps_result = streamer.update_caps("video/x-raw", 640, 360).await;
        assert!(update_caps_result.is_ok());

        sleep(Duration::from_secs(5)).await;

        eprintln!("Pausing streamer...");
        let pause_result = streamer.pause().await;
        assert!(pause_result.is_ok());

        sleep(Duration::from_secs(5)).await;

        eprintln!("Resuming streamer...");
        let start_result = streamer.start().await;
        assert!(start_result.is_ok());

        sleep(Duration::from_secs(5)).await;
        let stop_result = streamer.stop().await;
        assert!(stop_result.is_ok());
        sleep(Duration::from_secs(5)).await;
    }
}
