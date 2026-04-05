/// Create the GStreamer pipeline
use anyhow::{Result, anyhow};
use gstreamer as gst;
use gst::prelude::*;
use crate::video::streamer::{CAPSFILTER, PipelineFactory, create_decode_sink_chain};


pub struct V4L2Pipeline {
    pub device_path: String,
}

impl PipelineFactory for V4L2Pipeline {
    fn create_pipeline(&self) -> Result<gst::Pipeline> {
        let pipeline = gst::Pipeline::new();

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

        let decodebin = create_decode_sink_chain(&pipeline)?;
        // Add elements to pipeline
        pipeline.add_many([&source, &capfilter])?;
        
        // Link elements
        gst::Element::link_many([&source, &capfilter, &decodebin])?;

        Ok(pipeline)
    }
}