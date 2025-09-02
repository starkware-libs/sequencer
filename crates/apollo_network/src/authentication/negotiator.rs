use std::io::Error as IoError;

use async_trait::async_trait;
use futures::{Sink, Stream};
use libp2p::PeerId;

pub enum NegotiatorOutput {
    None,
    /// Returned when the handshake concluded that the currently connecting peer is a duplicate of
    /// the currently connected peer. `PeerId` is the (older) currently connected peer.
    DuplicatePeer(PeerId),
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NegotiatorError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Authentication failed")]
    AuthenticationFailed,
}

#[async_trait]
pub trait Negotiator: Send + Clone {
    /// Performs the handshake protocol when we are the incoming connection side.
    /// `connection` is the channel that can be used to communicate with the other peer.
    async fn negotiate_incoming_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut dyn ConnectionEndpoint,
    ) -> Result<NegotiatorOutput, NegotiatorError>;

    /// Performs the handshake protocol when we are the outgoing connection side.
    /// `connection` is the channel that can be used to communicate with the other peer.
    async fn negotiate_outgoing_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut dyn ConnectionEndpoint,
    ) -> Result<NegotiatorOutput, NegotiatorError>;

    /// A unique identified for your authentication protocol. For example: "strk_id" or
    /// "strk_id_v2".
    // TODO(guy.f): Consider making this a const.
    fn protocol_name(&self) -> &'static str;
}

#[async_trait::async_trait]
pub trait ConnectionEndpoint: Unpin + Send {
    /// Sends data over the connection.
    async fn send(&mut self, data: Vec<u8>) -> Result<(), IoError>;

    /// Receives data from the connection.
    async fn receive(&mut self) -> Result<Vec<u8>, IoError>;
}
