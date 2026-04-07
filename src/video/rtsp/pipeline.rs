use anyhow::{Result, anyhow};
use gstreamer as gst;
use gst::prelude::*;
use crate::video::streamer::{PipelineFactory, create_audio_sink_chain, create_decode_sink_chain};


pub struct RtspPipeline {
    pub name: String,
    pub url: String,
    pub protocol: String,
}

impl PipelineFactory for RtspPipeline {
    fn name(&self) -> &str {
        &self.name
    }

    fn create_pipeline(&self) -> Result<gst::Pipeline> {
        let pipeline = gst::Pipeline::with_name(&self.name);

        let source = gst::ElementFactory::make("rtspsrc")
            .name("source")
            .property("location", &self.url)
            .property_from_str("protocols", &self.protocol)
            .property("latency", 200u32)
            .build()
            .map_err(|e| anyhow!("Failed to create rtspsrc: {}", e))?;

        let decodebin = create_decode_sink_chain(&pipeline, &self.name)?;
        let audio_decodebin = create_audio_sink_chain(&pipeline)?;
        pipeline.add(&source)?;

        // rtspsrc has dynamic pads — link to decodebin when pad appears
        let decodebin_weak = decodebin.downgrade();
        let audio_decobin_weak = audio_decodebin.downgrade();
        source.connect_pad_added(move |_, src_pad| {
            // Only link video pads
            let caps = match src_pad.current_caps().or_else(|| Some(src_pad.query_caps(None))) {
                Some(c) => c,
                None => return,
            };
            let Some(structure) = caps.structure(0) else { return };
            if !structure.name().starts_with("application/x-rtp") {
                return;
            }
            // Check media type is video
            if let Ok(media) = structure.get::<String>("media") {
                match media.as_str() {
                    "video" => {
                        let Some(decodebin) = decodebin_weak.upgrade() else { return };
                        let sink_pad = decodebin.static_pad("sink").expect("Failed to get decodebin sink pad");
                        if sink_pad.is_linked() { return; }
                        if let Err(err) = src_pad.link(&sink_pad) {
                            eprintln!("Failed to link rtspsrc to decodebin: {}", err);
                        }
                    }
                    "audio" => {
                        let Some(audio_decodebin) = audio_decobin_weak.upgrade() else { return };
                        let sink_pad = audio_decodebin.static_pad("sink").expect("Failed to get audio decodebin sink pad");
                        if sink_pad.is_linked() { return; }
                        if let Err(err) = src_pad.link(&sink_pad) {
                            eprintln!("Failed to link rtspsrc audio pad to audio decodebin: {}", err);
                        }
                    }
                    _ => {}
                }
            }
        });

        Ok(pipeline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtsp_pipeline_name() {
        let pipeline = RtspPipeline {
            name: "Test Camera".to_string(),
            url: "rtsp://localhost/stream".to_string(),
            protocol: "tcp".to_string(),
        };
        assert_eq!(pipeline.name(), "Test Camera");
    }

    #[test]
    fn test_rtsp_pipeline_creation() {
        gst::init().unwrap();
        let factory = RtspPipeline {
            name: "Test Camera".to_string(),
            url: "rtsp://localhost/stream".to_string(),
            protocol: "tcp".to_string(),
        };
        let pipeline = factory.create_pipeline();
        assert!(pipeline.is_ok());
        let pipeline = pipeline.unwrap();
        assert!(pipeline.by_name("source").is_some());
    }

    #[test]
    fn test_rtsp_pipeline_protocol_udp() {
        gst::init().unwrap();
        let factory = RtspPipeline {
            name: "UDP Camera".to_string(),
            url: "rtsp://localhost/stream".to_string(),
            protocol: "udp".to_string(),
        };
        let pipeline = factory.create_pipeline();
        assert!(pipeline.is_ok());
    }
}