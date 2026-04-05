fn main() -> Result<(), Box<dyn std::error::Error>> {
    prost_build::compile_protos(&["src/video-device.proto"], &["src/"])?;
    Ok(())
}
