use asynchronous_codec::Framed;
use tokio_util::compat::TokioAsyncReadCompatExt;

use super::NegotiatorChannelCodec;

// OPTION A: A helper method for tests.

pub type TestNegotiatorChannel =
    Framed<tokio_util::compat::Compat<tokio::io::DuplexStream>, NegotiatorChannelCodec>;

/// Creates a pair of connected negotiator channels for testing.
pub fn create_negotiator_channel_pair() -> (TestNegotiatorChannel, TestNegotiatorChannel) {
    let (stream1, stream2) = tokio::io::duplex(1024);

    let framed1 = Framed::new(stream1.compat(), NegotiatorChannelCodec);
    let framed2 = Framed::new(stream2.compat(), NegotiatorChannelCodec);

    (framed1, framed2)
}

#[cfg(test)]
mod tests {
    use futures::{SinkExt, StreamExt};

    use super::*;

    #[tokio::test]
    async fn test_create_negotiator_channel_pair() {
        let (mut r1, mut r2) = create_negotiator_channel_pair();

        let data = vec![1u8];

        // Send data from r1 to r2
        r1.send(data.clone()).await.unwrap();

        // Receive data on r2
        let received = r2.next().await.unwrap().unwrap();

        assert_eq!(received, data);
    }
}

// OPTION B: A trait for sending/receving which you can mock in tests. I will wrap the frame object
// in this trait.

/// An async trait for connection endpoints that can send and receive data.
/// Implementations must be Unpin + Send for use in async contexts.
#[async_trait::async_trait]
pub trait ConnectionEndpoint: Unpin + Send {
    /// Sends data over the connection.
    async fn send(&mut self, data: Vec<u8>) -> Result<(), std::io::Error>;

    /// Receives data from the connection.
    async fn receive(&mut self) -> Result<Vec<u8>, std::io::Error>;
}
