/// Create the GStreamer pipeline
use anyhow::{Result, anyhow};
use gstreamer as gst;
use gst::prelude::*;
use crate::video::streamer::{CAPSFILTER, PipelineFactory, create_decode_sink_chain};


pub struct V4L2Pipeline {
    pub device_path: String,
}

impl PipelineFactory for V4L2Pipeline {
    fn name(&self) -> &str {
        &self.device_path
    }

    fn create_pipeline(&self) -> Result<gst::Pipeline> {
        let pipeline = gst::Pipeline::with_name(&self.device_path);

        // Create elements
        let source = gst::ElementFactory::make("v4l2src")
            .name("source")
            .property("device", self.device_path.clone())
            .build()
            .map_err(|e| anyhow!("Failed to create v4l2src: {}", e))?;

        let capfilter = gst::ElementFactory::make("capsfilter")
            .name(CAPSFILTER)
            .build()
            .map_err(|e| anyhow!("Failed to create capsfilter: {}", e))?;

        let decodebin = create_decode_sink_chain(&pipeline, &self.device_path)?;
        // Add elements to pipeline
        pipeline.add_many([&source, &capfilter])?;
        
        // Link elements
        gst::Element::link_many([&source, &capfilter, &decodebin])?;

        Ok(pipeline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v4l2_pipeline_name() {
        let pipeline = V4L2Pipeline { device_path: "/dev/video0".to_string() };
        assert_eq!(pipeline.name(), "/dev/video0");
    }

    #[test]
    fn test_v4l2_pipeline_creation() {
        gst::init().unwrap();
        let factory = V4L2Pipeline { device_path: "/dev/video99".to_string() };
        let pipeline = factory.create_pipeline();
        assert!(pipeline.is_ok());
        let pipeline = pipeline.unwrap();
        assert!(pipeline.by_name("source").is_some());
        assert!(pipeline.by_name(CAPSFILTER).is_some());
    }
}