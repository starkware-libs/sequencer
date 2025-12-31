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

/// This is a dummy implementation of the Negotiator trait used only so you can use
/// `Option<Negotiator>::None` (where you don't have a real type). Instances of this type should
/// never be created.
// We make it an enum to enforce that it is not possible to create an instance of this type.
// TODO(noam.s): Try to remove this when we use the ComposedNoiseConfig in the network manager.
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) enum DummyNegotiatorType {}

#[async_trait]
impl Negotiator for DummyNegotiatorType {
    type Error = IoError;

    fn protocol_name(&self) -> &'static str {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }

    async fn negotiate_connection(
        &mut self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection_sender: &mut ConnectionSender,
        _connection_receiver: &mut ConnectionReceiver,
        _side: NegotiationSide,
    ) -> Result<NegotiatorOutput, Self::Error> {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }
}
