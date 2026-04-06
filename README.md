# Remux

A Rust-based video device manager that discovers, monitors, and streams from V4L2 and RTSP cameras. Uses GStreamer for video pipelines and Zenoh for distributed pub/sub communication, enabling remote control and distribution of video streams across a network.

## Features

- **V4L2 Camera Detection**: Automatically discovers local video capture devices with periodic scanning
- **RTSP Camera Support**: Connects to network IP cameras via RTSP (configurable protocol TCP/UDP)
- **Extensible Pipeline Architecture**: Trait-based `PipelineFactory` design for easy addition of new camera types
- **Distributed Communication**: Publishes device information and receives stream control commands via Zenoh
- **Configuration File**: TOML-based configuration for RTSP cameras
- **Shell Completions**: Auto-generated bash/zsh/fish completions via `--completions`
- **Protocol Buffers**: Structured serialization for device metadata and stream control messages

## Requirements

- Rust 2024 edition or later
- GStreamer 1.0 development libraries
- Linux (V4L2 support)

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

```bash
git clone <repository-url>
cd remux
cargo build --release
```

## Usage

### Basic Usage

Start with V4L2 camera auto-detection only:
```bash
remux
```

### With RTSP Cameras

```bash
remux --config /path/to/config.toml
```

If no `--config` is provided, remux looks for `/etc/remux/config.toml`. If the file doesn't exist, only V4L2 cameras are used.

### Configuration File

```toml
[[rtsp]]
name = "Camera Entrance"
uri = "rtsp://admin:password@192.168.1.23/cam/realmonitor?channel=1&subtype=0"
protocol = "tcp"

[[rtsp]]
name = "Camera Parking"
uri = "rtsp://admin:password@192.168.1.24/stream1"
# protocol defaults to "udp"
```

### Shell Completions

```bash
remux --completions bash > /etc/bash_completion.d/remux
remux --completions zsh > ~/.zfunc/_remux
remux --completions fish > ~/.config/fish/completions/remux.fish
```

## Architecture

```
src/
├── main.rs                     # CLI entry point (clap)
├── app.rs                      # Event loop and application orchestration
├── config.rs                   # TOML configuration parsing
├── video/
│   ├── mod.rs                  # Shared protobuf types
│   ├── streamer.rs             # Generic Streamer + PipelineFactory trait
│   ├── v4l2/
│   │   ├── device_monitor.rs   # GStreamer V4L2 device scanning
│   │   └── pipeline.rs         # V4L2 pipeline (v4l2src)
│   └── rtsp/
│       └── pipeline.rs         # RTSP pipeline (rtspsrc)
├── com/
│   └── service.rs              # Zenoh pub/sub service
└── video-device.proto          # Protobuf schema
```

### Data Flow

1. **V4L2**: `DeviceMonitor` scans for local cameras every 5 seconds
2. **RTSP**: Cameras are loaded from the configuration file at startup
3. **Streaming**: Each camera gets a dedicated `Streamer` thread with its own GStreamer pipeline
4. **Zenoh**: Device lists are published to `video/devices`, stream control commands are received on `video/stream_control`

## Zenoh Topics

| Topic                  | Direction | Description                                    |
|------------------------|-----------|------------------------------------------------|
| `video/devices`        | Publish   | Protobuf-encoded list of detected V4L2 devices |
| `video/stream_control` | Subscribe | Start/stop/configure individual streams        |

## Dependencies

| Crate        | Purpose                        |
|--------------|--------------------------------|
| gstreamer    | Video pipeline management      |
| zenoh        | Distributed pub/sub messaging  |
| prost        | Protocol Buffers serialization |
| clap         | CLI argument parsing           |
| serde + toml | Configuration file parsing     |
| tokio        | Async runtime                  |

## Troubleshooting

### No V4L2 devices detected
- Check devices exist: `ls -la /dev/video*`
- Verify GStreamer: `gst-inspect-1.0 v4l2src`
- Check permissions: user may need to be in the `video` group

### RTSP camera not connecting
- Test with ffplay: `ffplay -rtsp_transport tcp rtsp://user:pass@ip/path`
- Try `protocol = "tcp"` in config (UDP may be blocked by firewalls/VPNs)

### Build errors
- Install GStreamer development packages (see Requirements)
- Update Rust: `rustup update`

## License

This project is licensed under the [GNU Affero General Public License v3.0](LICENSE).

## See Also

- [GStreamer Documentation](https://gstreamer.freedesktop.org/documentation/)
- [Zenoh Documentation](https://zenoh.io/docs/)
- [gst-plugin-zenoh](https://github.com/p13marc/gst-plugin-zenoh) - GStreamer plugin for streaming via Zenoh
