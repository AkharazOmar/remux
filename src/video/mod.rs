// Include the generated protobuf code
pub mod video_device {
    include!(concat!(env!("OUT_DIR"), "/video_device.rs"));
}

pub mod v4l2;
pub mod rtsp;
pub mod streamer;