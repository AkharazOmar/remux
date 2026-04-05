pub struct Service {
    session: zenoh::Session,
    video_device_publisher: zenoh::pubsub::Publisher<'static>,
}

impl Service {
    /// Create a new Service instance
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let session = zenoh::open(zenoh::Config::default()).await.unwrap();
        let video_device_publisher = session.declare_publisher("video/devices").await.unwrap();
        Ok(Self { session, video_device_publisher })
    }

    pub fn video_devices_put(&self, data: Vec<u8>) -> zenoh::pubsub::PublisherPutBuilder<'_> {
        self.video_device_publisher.put(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_creation() {
        let service = Service::new().await;
        assert!(service.is_ok());
    }
}