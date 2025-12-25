use std::io::Error as IoError;

use async_trait::async_trait;
use futures::{Sink, Stream};
use libp2p::PeerId;
#[cfg(any(feature = "testing", test))]
use mockall::automock;

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

pub type ConnectionSender = dyn Sink<Vec<u8>, Error = IoError> + Unpin + Send;
pub type ConnectionReceiver = dyn Stream<Item = Result<Vec<u8>, IoError>> + Unpin + Send;

#[async_trait]
#[cfg_attr(any(feature = "testing", test), automock(type Error = std::io::Error;))]
pub trait Negotiator: Send + Clone {
    type Error: std::error::Error + Send + Sync;

    /// Performs the handshake protocol.
    /// `connection_sender` is the channel that can be used to send data to the remote peer.
    /// `connection_receiver` is the channel that can be used to receive data from the remote peer.
    async fn negotiate_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection_sender: &mut ConnectionSender,
        connection_receiver: &mut ConnectionReceiver,
        side: NegotiationSide,
    ) -> Result<NegotiatorOutput, Self::Error>;

    /// A unique identified for your authentication protocol. For example: "strk_id" or
    /// "strk_id_v2".
    // TODO(noam.s): Consider making this a const.
    fn protocol_name(&self) -> &'static str;
}

// Automock does not implement Clone, so we need to do it manually.
#[cfg(any(feature = "testing", test))]
impl Clone for MockNegotiator {
    fn clone(&self) -> Self {
        MockNegotiator::default()
    }
}
