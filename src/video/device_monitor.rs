use anyhow::Result;
use gstreamer as gst;
use gst::prelude::*;
use std::sync::{Arc, Mutex};
use tokio::task;

// Include the generated protobuf code
pub mod video_device {
    include!(concat!(env!("OUT_DIR"), "/video_device.rs"));
}

use video_device::{VideoDevice, VideoDeviceList, VideoFormat, DeviceProperty};

#[derive(Clone)]
pub struct DeviceMonitor {
    device_monitor: gst::DeviceMonitor,
    last_device_list: Arc<Mutex<Option<VideoDeviceList>>>,
}

impl DeviceMonitor {
    /// Creates a new VideoDeviceMonitor instance
    pub fn new() -> Result<Self> {
        gst::init()?;

        let device_monitor = gst::DeviceMonitor::new();
        device_monitor.add_filter(Some("Video/Source"), None);

        Ok(Self {
            device_monitor,
            last_device_list: Arc::new(Mutex::new(None))
        })
    }

    /// Scans for video devices and returns them as a protobuf message if changed
    pub async fn scan_devices(&self) -> Result<Option<VideoDeviceList>> {
        let device_monitor = self.device_monitor.clone();
        let last = Arc::clone(&self.last_device_list);

        let device_list = task::spawn_blocking(move || -> Result<VideoDeviceList, anyhow::Error> {
            device_monitor.start().map_err(|e| anyhow::anyhow!("Failed to start device monitor: {}", e))?;

            // Get the list of devices
            let raw_devices = device_monitor.devices();

            let mut device_list = VideoDeviceList { devices: vec![] };

            for device in raw_devices {
                // Assuming parse_device is implemented
                let video_device = Self::parse_device(&device)?;
                device_list.devices.push(video_device);
            }

            device_monitor.stop();

            Ok(device_list)
        }).await??;

        let mut last_guard = last.lock().unwrap();
        if *last_guard != Some(device_list.clone()) {
            *last_guard = Some(device_list.clone());
            Ok(Some(device_list))
        } else {
            Ok(None)
        }
    }

    /// Parses a GStreamer device into a protobuf VideoDevice
    fn parse_device(device: &gst::Device) -> Result<VideoDevice> {
        let name = device.display_name().to_string();
        let device_class = device.device_class().to_string();

        // Get device path from properties
        let properties = device.properties();
        let device_path = properties
            .as_ref()
            .and_then(|props| {
                props.get::<String>("api.v4l2.path").ok()
                    .or_else(|| props.get::<String>("device.path").ok())
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Parse properties
        let mut proto_properties = vec![];
        if let Some(props) = properties {
            for (key, value) in props.iter() {
                if let Ok(val_str) = value.get::<String>() {
                    proto_properties.push(DeviceProperty {
                        key: key.to_string(),
                        value: val_str,
                    });
                } else if let Ok(val_str) = value.serialize() {
                    proto_properties.push(DeviceProperty {
                        key: key.to_string(),
                        value: val_str.to_string(),
                    });
                }
            }
        }

        // Get supported formats
        let caps = device.caps();
        let mut formats = vec![];

        if let Some(caps) = caps {
            for i in 0..caps.size() {
                if let Some(structure) = caps.structure(i) {
                    let format = Self::parse_format(&structure);
                    formats.push(format);
                }
            }
        }

        Ok(VideoDevice {
            name,
            device_path,
            device_class,
            formats,
            properties: proto_properties,
        })
    }

    /// Parses a GStreamer structure into a VideoFormat
    fn parse_format(structure: &gst::StructureRef) -> VideoFormat {
        let mime_type = structure.name().to_string();

        let width = structure
            .get::<i32>("width")
            .unwrap_or(0);

        let height = structure
            .get::<i32>("height")
            .unwrap_or(0);

        let format = structure
            .get::<String>("format")
            .unwrap_or_else(|_| "unknown".to_string());

        // Try to get framerate
        let (framerate_num, framerate_den) = structure
            .get::<gst::Fraction>("framerate")
            .map(|f| (f.numer(), f.denom()))
            .unwrap_or((0, 1));

        VideoFormat {
            mime_type,
            width,
            height,
            format,
            framerate_num,
            framerate_den,
        }
    }
}

impl Default for DeviceMonitor {
    fn default() -> Self {
        Self::new().expect("Failed to create VideoDeviceMonitor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scan_devices() {
        // FIXME: Use mock devices for testing
        let monitor = DeviceMonitor::new().expect("Failed to create VideoDeviceMonitor");
        let device_list = monitor.scan_devices().await.expect("Failed to scan devices")
            .expect("No devices found");

        println!("Found {} video device(s):", device_list.devices.len());
        assert_eq!(device_list.devices.len(), 1);
        for device in &device_list.devices {
            println!("Device Name: {}", device.name);
            println!("Device Path: {}", device.device_path);
            println!("Device Class: {}", device.device_class);
            println!("Formats:");
            for format in &device.formats {
                println!(
                    "  - {} {}x{} {} ({}fps)",
                    format.mime_type,
                    format.width,
                    format.height,
                    format.format,
                    if format.framerate_den > 0 {
                        format.framerate_num / format.framerate_den
                    } else {
                        0
                    }
                );
            }
            println!("Properties:");
            for prop in &device.properties {
                println!("  - {}: {}", prop.key, prop.value);
            }
            println!();
        }
    }
}