use anyhow::Result;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use prost::Message;
use crate::video::device_monitor::{DeviceMonitor, video_device::VideoDeviceList};
use crate::com::service::{Service};

/// Application events
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AppEvent {
    /// Device list changed
    DevicesChanged(VideoDeviceList),
    /// Scan for devices
    ScanDevices,
    /// Shutdown the application
    Shutdown,
}

/// Main application structure
pub struct App {
    event_tx: mpsc::Sender<AppEvent>,
    event_rx: mpsc::Receiver<AppEvent>,
    device_monitor: DeviceMonitor,
    service: Service,
}

impl App {
    /// Create a new application instance
    pub async fn new() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(100);
        let device_monitor = DeviceMonitor::new()?;

        // Create Zenoh session
        let service = Service::new().await.unwrap();
        Ok(Self {
            event_tx,
            event_rx,
            device_monitor,
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
                AppEvent::Shutdown => {
                    println!("Shutting down...");
                    break;
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
    async fn handle_devices_changed(&self, device_list: VideoDeviceList) -> Result<()> {
        println!("\n=== Video Devices Update ===");
        println!("Found {} device(s)\n", device_list.devices.len());

        for (idx, device) in device_list.devices.iter().enumerate() {
            println!("Device #{}", idx + 1);
            println!("  Name: {}", device.name);
            println!("  Path: {}", device.device_path);
            println!("  Class: {}", device.device_class);
            println!("  Formats: {} available", device.formats.len());
            println!();
        }

        // Publish to Zenoh
        let mut buf = vec![];
        device_list.encode(&mut buf)?;
        self.service.video_devices_put(buf).await
            .map_err(|e| anyhow::anyhow!("Failed to publish to Zenoh: {}", e))?;

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
}
