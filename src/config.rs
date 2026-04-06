use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub rtsp: Vec<RtspCamera>,
}

#[derive(Debug, Deserialize)]
pub struct RtspCamera {
    pub name: String,
    pub uri: String,
    #[serde(default="default_protocol")]
    pub protocol: String,
}

fn default_protocol() -> String {
    "udp".into()
}

impl Config {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }
}