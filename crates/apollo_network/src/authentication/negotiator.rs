use std::io::Error as IoError;

use async_trait::async_trait;
use futures::{Sink, Stream};
use libp2p::PeerId;

pub enum NegotiationSide {
    Inbound,
    Outbound,
}

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
    /// Performs the handshake protocol.
    /// `connection_sender` is the channel that can be used to send data to the remote peer.
    /// `connection_receiver` is the channel that can be used to receive data from the remote peer.
    async fn negotiate_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection_sender: &mut (dyn Sink<Vec<u8>, Error = IoError> + Unpin + Send),
        connection_receiver: &mut (dyn Stream<Item = Result<Vec<u8>, IoError>> + Unpin + Send),
        side: NegotiationSide,
    ) -> Result<NegotiatorOutput, NegotiatorError>;

    /// A unique identified for your authentication protocol. For example: "strk_id" or
    /// "strk_id_v2".
    // TODO(noam.s): Consider making this a const.
    fn protocol_name(&self) -> &'static str;
}
