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

/// Streamer events sent through the event channel
#[derive(Debug, Clone)]
pub enum StreamerEvent {
    StateChanged(StreamerState),
    EndOfStream,
    Error(String),
    BufferAvailable(Vec<u8>),
}

/// Commands sent to the GStreamer thread
#[derive(Debug)]
enum GstCommand {
    Start,
    Stop,
    Pause,
    Shutdown,
}

/// Video streamer using GStreamer
pub struct Streamer {
    command_tx: mpsc::Sender<GstCommand>,
    state: Arc<Mutex<StreamerState>>,
}

impl Streamer {
    /// Create a new streamer for a V4L2 device
    pub fn new(device_path: &str) -> Result<Self> {
        let state = Arc::new(Mutex::new(StreamerState::Stopped));
        let (command_tx, command_rx) = mpsc::channel::<GstCommand>(32);

        // Clone for the GStreamer thread
        let device_path = device_path.to_string();
        let state_clone = Arc::clone(&state);

        // Spawn dedicated GStreamer thread
        std::thread::spawn(move || {
            Self::gstreamer_thread(device_path, command_rx, state_clone);
        });

        Ok(Self {
            command_tx,
            state,
        })
    }

    /// GStreamer thread - handles all GStreamer operations
    fn gstreamer_thread(
        device_path: String,
        command_rx: mpsc::Receiver<GstCommand>,
        state: Arc<Mutex<StreamerState>>,
    ) {
        // Initialize GStreamer in this thread
        if let Err(e) = gst::init() {
            eprintln!("Failed to initialize GStreamer: {}", e);
            return;
        }

        // Create main loop with default context
        let main_loop = glib::MainLoop::new(None, false);
        let main_loop_clone = main_loop.clone();

        // Create pipeline
        let pipeline = match Self::create_pipeline(&device_path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to create pipeline: {}", e);
                return;
            }
        };

        // Set up bus watch
        let _bus_watch = if let Some(bus) = pipeline.bus() {
            let state_clone = Arc::clone(&state);
            let main_loop_for_bus = main_loop_clone.clone();

            Some(
                bus.add_watch_local(move |_, msg| {
                    Self::handle_bus_message(
                        msg,
                        &state_clone,
                        &main_loop_for_bus,
                    )
                })
                .expect("Failed to add bus watch"),
            )
        } else {
            None
        };

        // Set up command receiver using idle callback
        let command_rx = Arc::new(std::sync::Mutex::new(command_rx));
        let pipeline_clone = pipeline.clone();
        let state_clone = Arc::clone(&state);

        glib::idle_add_local(move || {
            let cmd = {
                let mut rx = command_rx.lock().unwrap();
                rx.try_recv().ok()
            };

            if let Some(cmd) = cmd {
                Self::handle_command(
                    &cmd,
                    &pipeline_clone,
                    &state_clone,
                    &main_loop_clone,
                );
            }

            glib::ControlFlow::Continue
        });

        // Run the main loop
        main_loop.run();

        // Cleanup
        let _ = pipeline.set_state(gst::State::Null);
    }

    /// Create the GStreamer pipeline
    fn create_pipeline(device_path: &str) -> Result<gst::Pipeline> {
        let pipeline = gst::Pipeline::new();

        // Create elements
        let source = gst::ElementFactory::make("v4l2src")
            .property("device", device_path)
            .build()
            .map_err(|e| anyhow!("Failed to create v4l2src: {}", e))?;

        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .map_err(|e| anyhow!("Failed to create videoconvert: {}", e))?;

        let sink = gst::ElementFactory::make("autovideosink")
            .build()
            .map_err(|e| anyhow!("Failed to create autovideosink: {}", e))?;

        // Add elements to pipeline
        pipeline.add_many([&source, &convert, &sink])?;

        // Link elements
        gst::Element::link_many([&source, &convert, &sink])?;

        Ok(pipeline)
    }

    /// Handle a command from the command channel
    fn handle_command(
        cmd: &GstCommand,
        pipeline: &gst::Pipeline,
        state: &Arc<Mutex<StreamerState>>,
        main_loop: &glib::MainLoop,
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
            GstCommand::Shutdown => {
                let _ = pipeline.set_state(gst::State::Null);
                main_loop.quit();
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

    /// Get current state
    pub fn get_state(&self) -> StreamerState {
        self.state.lock().unwrap().clone()
    }
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