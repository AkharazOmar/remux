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
    use std::time::Duration;

    use super::*;
    use zenoh::handlers::FifoChannel;

    #[tokio::test]
    async fn test_service_creation() {
        let service = Service::new().await;
        assert!(service.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_video_devices_put() {
        let service = Service::new().await.unwrap();
        let mut subscriber = service
            .session
            .declare_subscriber("video/devices")
            .with(FifoChannel::new(1))
            .await
            .unwrap();

        let data = vec![1, 2, 3, 4];
        service.video_devices_put(data.clone()).await.unwrap();

        let sample = tokio::time::timeout(Duration::from_secs(2), subscriber.recv_async())
            .await
            .expect("Timed out waiting for zenoh publication")
            .unwrap();
        assert_eq!(sample.payload().to_bytes().as_ref(), data.as_slice());

        subscriber.undeclare().await.unwrap();
    }
}
