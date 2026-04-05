use anyhow::{Result, anyhow};
use gstreamer as gst;
use gst::prelude::*;
use crate::video::streamer::{PipelineFactory, create_decode_sink_chain};


pub struct RtspPipeline {
    pub url: String,
}

impl PipelineFactory for RtspPipeline {
    fn create_pipeline(&self) -> Result<gst::Pipeline> {
        let pipeline = gst::Pipeline::new();

        // Create elements
        let source = gst::ElementFactory::make("rtspsrc")
            .name("source")
            .property("location", &self.url)
            .build()
            .map_err(|e| anyhow!("Failed to create rtspsrc: {}", e))?;
        let depay = gst::ElementFactory::make("rtph264depay")
            .name("depay")
            .build()
            .map_err(|e| anyhow!("Failed to create rtph264depay: {}", e))?;

        let depay_weak = depay.downgrade();
        source.connect_pad_added(move |_, src_pad| {
            // Handle pad added event
            let Some(depay) = depay_weak.upgrade() else {
                println!("Depay element was dropped");
                return;
            };
            let sink_pad = depay.static_pad("sink").expect("Failed to get depay sink pad");
            if sink_pad.is_linked() {
                println!("Sink pad already linked");
                return;
            }
            let src_pad = src_pad.clone();
            if let Err(err) = src_pad.link(&sink_pad) {
                println!("Failed to link source pad to depay sink pad: {}", err);
            } else {
                println!("Linked source pad to depay sink pad");
            }
        });

        let decobin = create_decode_sink_chain(&pipeline)?;
        // Add elements to pipeline
        pipeline.add_many([&source, &depay])?;
        gst::Element::link_many([&source, &depay, &decobin])?;

        Ok(pipeline)
    }
}