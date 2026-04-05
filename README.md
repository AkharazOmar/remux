# Remux

A Rust-based video device monitoring library that uses GStreamer to detect and enumerate V4L2 cameras, with Protocol Buffers for structured data representation.

## Features

- **V4L2 Camera Detection**: Automatically discovers all video capture devices using GStreamer's device monitor
- **Format Enumeration**: Lists all supported video formats, resolutions, and framerates for each camera
- **Device Properties**: Extracts detailed device metadata (driver, bus info, capabilities, etc.)
- **Protocol Buffers**: Structured data representation for easy serialization and cross-platform compatibility
- **Type-Safe**: Leverages Rust's type system for reliable camera enumeration

## Requirements

- Rust 2024 edition or later
- GStreamer 1.0 development libraries
- V4L2 compatible system (Linux)

### System Dependencies

On Debian/Ubuntu:
```bash
sudo apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
```

On Fedora:
```bash
sudo dnf install gstreamer1-devel gstreamer1-plugins-base-devel
```

On Arch Linux:
```bash
sudo pacman -S gstreamer gst-plugins-base
```

## Installation

Clone the repository:
```bash
git clone <repository-url>
cd remux
```

Build the project:
```bash
cargo build --release
```

## Usage

### Running the Example

To scan and display all detected video devices:
```bash
cargo run
```

### Using as a Library

Add to your `Cargo.toml`:
```toml
[dependencies]
remux = { path = "/path/to/remux" }
```

Example code:
```rust
use remux::video_device_monitor::VideoDeviceMonitor;

fn main() -> anyhow::Result<()> {
    // Create a video device monitor
    let monitor = VideoDeviceMonitor::new()?;

    // Scan for devices
    let device_list = monitor.scan_devices()?;

    // Access device information
    for device in device_list.devices {
        println!("Camera: {}", device.name);
        println!("Path: {}", device.device_path);

        for format in device.formats {
            println!("  Format: {}x{} {} @ {}fps",
                format.width,
                format.height,
                format.format,
                format.framerate_num / format.framerate_den.max(1)
            );
        }
    }

    Ok(())
}
```

## Project Structure

```
remux/
├── src/
│   ├── main.rs                    # Example application
│   ├── video_device_monitor.rs    # Core monitoring logic
│   └── video-device.proto         # Protocol Buffer definitions
├── build.rs                       # Build script for protobuf compilation
├── Cargo.toml                     # Project dependencies
└── README.md                      # This file
```

## Protocol Buffer Schema

The project uses Protocol Buffers to represent video device information:

### VideoDevice
- `name`: Display name of the camera
- `device_path`: System path (e.g., `/dev/video0`)
- `device_class`: Device class (typically "Video/Source")
- `formats`: List of supported video formats
- `properties`: Device-specific properties

### VideoFormat
- `mime_type`: MIME type (e.g., "video/x-raw", "image/jpeg")
- `width`: Video width in pixels
- `height`: Video height in pixels
- `format`: Pixel format (e.g., "YUY2", "MJPG", "I420")
- `framerate_num`: Framerate numerator
- `framerate_den`: Framerate denominator

### DeviceProperty
- `key`: Property name
- `value`: Property value as string

## API Documentation

### VideoDeviceMonitor

#### `new() -> Result<Self>`
Creates a new video device monitor instance. Initializes GStreamer and sets up device filtering for V4L2 cameras.

#### `scan_devices(&self) -> Result<VideoDeviceList>`
Scans the system for available video devices and returns a structured list of all detected cameras with their capabilities.

## Dependencies

- **gstreamer** (0.24.4): GStreamer bindings for Rust
- **prost** (0.14.1): Protocol Buffers implementation
- **bytes** (1.9.0): Byte buffer utilities
- **anyhow** (1.0): Error handling
- **zenoh** (1.7.1): Zero-overhead pub/sub networking (for future features)

### Build Dependencies
- **prost-build** (0.14.1): Protocol Buffer compiler

## Example Output

```
Scanning for video devices...

Found 1 video device(s):

Device #1
  Name: Laptop Webcam Module (2nd Gen) (V4L2)
  Path: /dev/video0
  Class: Video/Source
  Formats:
    - image/jpeg 1920x1080 unknown (30fps)
    - image/jpeg 1280x720 unknown (30fps)
    - video/x-raw 640x480 YUY2 (30fps)
  Properties:
    - api.v4l2.path: /dev/video0
    - device.api: v4l2
    - api.v4l2.cap.driver: uvcvideo
    ...
```

## Future Features

- [ ] Real-time device hotplug detection
- [ ] Camera capability testing
- [ ] Video streaming over Zenoh
- [ ] Remote camera access
- [ ] Multi-camera synchronization
- [ ] Format conversion utilities

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

[Add your license here]

## Troubleshooting

### No devices detected
- Ensure V4L2 devices exist: `ls -la /dev/video*`
- Check GStreamer installation: `gst-inspect-1.0 v4l2src`
- Verify user permissions: You may need to be in the `video` group

### Build errors
- Install GStreamer development packages (see Requirements)
- Update Rust: `rustup update`

## See Also

- [GStreamer Documentation](https://gstreamer.freedesktop.org/documentation/)
- [Protocol Buffers](https://developers.google.com/protocol-buffers)
- [V4L2 API](https://www.kernel.org/doc/html/latest/userspace-api/media/v4l/v4l2.html)
