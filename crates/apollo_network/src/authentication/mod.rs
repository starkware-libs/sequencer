use std::fmt::Debug;

use apollo_network_types::network_types::PeerId;
use async_trait::async_trait;
use futures::{Sink, SinkExt, Stream, StreamExt};
use thiserror::Error;

pub type Bytes = Vec<u8>;

/// Represents an entity that can perform an authentication protocol.
#[async_trait]
pub trait AuthNegotiator {
    // TODO(Elin): consider adding a message associated type.
    type Error: Debug;

    /// Performs a one-sided authentication protocol.
    async fn negotiate<S, R>(
        &self,
        self_peer_id: PeerId,
        other_peer_id: PeerId,
        mut connection: ConnectionEnd<S, R>,
    ) -> Result<bool, Self::Error>
    where
        S: Sink<Bytes> + Unpin + Send,
        R: Stream<Item = Bytes> + Unpin + Send;

    // TODO(Elin): consider separating incoming and outgoing negotiation methods.
}

/// A connection end is a pair of a sender and a receiver, held by one side of the connection.
pub struct ConnectionEnd<S, R> {
    pub sender: S,
    pub receiver: R,
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("Failed to receive message.")]
    Recv,
    #[error("Failed to send message.")]
    Send,
}

impl<S, R> ConnectionEnd<S, R>
where
    S: Sink<Bytes> + Unpin,
    R: Stream<Item = Bytes> + Unpin,
{
    pub async fn send(&mut self, message: Bytes) -> Result<(), ConnectionError> {
        self.sender.send(message).await.map_err(|_| ConnectionError::Send)
    }

    pub async fn recv(&mut self) -> Result<Bytes, ConnectionError> {
        self.receiver.next().await.ok_or(ConnectionError::Recv)
    }
}
