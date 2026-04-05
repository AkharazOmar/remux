mod video_device_monitor;

use video_device_monitor::VideoDeviceMonitor;

fn main() -> anyhow::Result<()> {
    let mut monitor = VideoDeviceMonitor::new()?;
    let devices = monitor.scan_devices()?;
    println!("Detected video devices: {:?}", devices);
    Ok(())
}
