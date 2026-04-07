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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_valid_config() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, r#"
[[rtsp]]
name = "Camera 1"
uri = "rtsp://admin:pass@192.168.1.10/stream1"
protocol = "tcp"

[[rtsp]]
name = "Camera 2"
uri = "rtsp://admin:pass@192.168.1.20/stream1"
"#).unwrap();

        let config = Config::load_from_file(file.path()).unwrap();
        assert_eq!(config.rtsp.len(), 2);
        assert_eq!(config.rtsp[0].name, "Camera 1");
        assert_eq!(config.rtsp[0].uri, "rtsp://admin:pass@192.168.1.10/stream1");
        assert_eq!(config.rtsp[0].protocol, "tcp");
        assert_eq!(config.rtsp[1].name, "Camera 2");
        assert_eq!(config.rtsp[1].protocol, "udp"); // default
    }

    #[test]
    fn test_load_config_empty() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "").unwrap();

        let config = Config::load_from_file(file.path()).unwrap();
        assert!(config.rtsp.is_empty());
    }

    #[test]
    fn test_load_config_missing_file() {
        let result = Config::load_from_file("/nonexistent/path/config.toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_invalid_toml() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "this is not valid toml [[[").unwrap();

        let result = Config::load_from_file(file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_default_protocol() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, r#"
[[rtsp]]
name = "Camera"
uri = "rtsp://localhost/stream"
"#).unwrap();

        let config = Config::load_from_file(file.path()).unwrap();
        assert_eq!(config.rtsp[0].protocol, "udp");
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.rtsp.is_empty());
    }
}