use std::fmt::Debug;

use apollo_network_types::network_types::PeerId;
use async_trait::async_trait;
use futures::{Sink, SinkExt, Stream, StreamExt};
use thiserror::Error;

use crate::Bytes;

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("Failed to receive message.")]
    Recv,
    #[error("Failed to send message.")]
    Send,
}

/// A connection end, held by one side of the connection.
/// This trait eliminates type duplication, since aliasing trait bounds is still unstable.
#[async_trait]
pub trait ConnectionEnd: Sink<Bytes> + Stream<Item = Bytes> + Unpin + Send {
    async fn send_message(&mut self, message: Bytes) -> Result<(), ConnectionError> {
        self.send(message).await.map_err(|_| ConnectionError::Send)
    }

    async fn receive_message(&mut self) -> Result<Bytes, ConnectionError> {
        self.next().await.ok_or(ConnectionError::Recv)
    }
}

/// Represents an entity that can perform an authentication protocol.
#[async_trait]
pub trait AuthNegotiator: Send + Clone {
    type Error: Debug;

    /// Performs a one-sided authentication protocol of the incoming part.
    async fn negotiate_incoming_connection<C: ConnectionEnd>(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut C,
    ) -> Result<bool, Self::Error>;

    /// Performs a one-sided authentication protocol of the outgoing part.
    async fn negotiate_outgoing_connection<C: ConnectionEnd>(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut C,
    ) -> Result<bool, Self::Error>;

    fn protocol_name(&self) -> &'static str;
}

// /// Represents an entity that can perform an authentication protocol.
// #[async_trait]
// pub trait Negotiator {
//     // TODO(Elin): consider adding a message associated type.
//     type Error: Debug;

//     /// Performs a one-sided authentication protocol.
//     async fn negotiate<S, R>(
//         &self,
//         self_peer_id: PeerId,
//         other_peer_id: PeerId,
//         mut connection: ConnectionEnd<S, R>,
//     ) -> Result<bool, Self::Error>
//     where
//         S: Sink<Bytes> + Unpin + Send,
//         R: Stream<Item = Bytes> + Unpin + Send;

//     // TODO(Elin): consider separating incoming and outgoing negotiation methods.
// }
