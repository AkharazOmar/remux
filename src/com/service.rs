pub struct Service {
    pub session: zenoh::Session,
    video_device_publisher: zenoh::pubsub::Publisher<'static>,
    pub stream_control_subscriber: zenoh::pubsub::Subscriber<zenoh::handlers::FifoChannelHandler<zenoh::sample::Sample>>,
}

impl Service {
    /// Create a new Service instance
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let session = zenoh::open(zenoh::Config::default()).await.unwrap();
        let video_device_publisher = session.declare_publisher("video/devices").await.unwrap();
        let stream_control_subscriber = session
            .declare_subscriber("video/stream_control")
            .with(zenoh::handlers::FifoChannel::new(100))
            .await
            .unwrap();

        Ok(Self { session, video_device_publisher, stream_control_subscriber })
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
        let subscriber = service
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_stream_control_subscription() {
        let session = zenoh::open(zenoh::Config::default()).await.unwrap();
        let publisher = session
            .declare_publisher("video/stream_control")
            .await
            .unwrap();
        publisher
            .put("Test Stream Control Message".as_bytes().to_vec())
            .await
            .unwrap();
    }
}