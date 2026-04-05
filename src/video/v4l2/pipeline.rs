/// Create the GStreamer pipeline
use anyhow::{Result, anyhow};
use gstreamer as gst;
use gst::prelude::*;
use crate::video::streamer::{PipelineFactory, CAPSFILTER};


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

        let decobin = gst::ElementFactory::make("decodebin")
            .build()
            .map_err(|e| anyhow!("Failed to create decodebin: {}", e))?;

        let videoconvert = gst::ElementFactory::make("videoconvert")
            .build()
            .map_err(|e| anyhow!("Failed to create videoconvert: {}", e))?;

        let sink = gst::ElementFactory::make("autovideosink")
            .build()
            .map_err(|e| anyhow!("Failed to create autovideosink: {}", e))?;

        // Add elements to pipeline
        pipeline.add_many([&source, &capfilter, &decobin, &videoconvert, &sink])?;

        // Link elements
        gst::Element::link_many([&source, &capfilter, &decobin])?;
        gst::Element::link_many([&videoconvert, &sink])?;

        let videoconvert_weak = videoconvert.downgrade();
        decobin.connect_pad_added(move |_dbin, src_pad| {
            let Some(videoconvert) = videoconvert_weak.upgrade() else {
                return;
            };
            let sink_pad = videoconvert.static_pad("sink").expect("Failed to get sink pad from videoconvert");
            if sink_pad.is_linked() {
                eprintln!("Sink pad already linked, ignoring");
                return;
            }

            let src_pad_caps = src_pad.current_caps().or_else(|| Some(src_pad.query_caps(None)));
            let Some(src_pad_caps) = src_pad_caps else {
                eprintln!("Failed to get caps from src pad");
                return;
            };
            let Some(src_pad_struct) = src_pad_caps.structure(0) else {return;};
            let src_pad_type = src_pad_struct.name();

            if src_pad_type.starts_with("video/") {
                match src_pad.link(&sink_pad) {
                    Ok(_) => eprintln!("Linked decodebin src pad to videoconvert sink pad"),
                    Err(e) => eprintln!("Failed to link decodebin src pad to videoconvert sink pad: {}", e),
                }
            }
        });

        Ok(pipeline)
    }
}