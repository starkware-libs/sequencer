use std::io::Error as IoError;

use async_trait::async_trait;
use libp2p::PeerId;
#[cfg(test)]
use mockall::automock;
use prost::Message;

pub enum NegotiationSide {
    Inbound,
    Outbound,
}

// TODO(noam.s): Remove this enum if we end up only having the Success case.
pub enum NegotiatorOutput {
    Success,
}

#[async_trait]
pub trait ConnectionSender<M>: Unpin + Send {
    async fn send(&mut self, data: M) -> Result<(), IoError>;
}

#[async_trait]
pub trait ConnectionReceiver<M>: Unpin + Send {
    async fn receive(&mut self) -> Result<M, IoError>;
}

#[async_trait]
#[cfg_attr(
    test,
    automock(
        type Error = std::io::Error;
        type WireMessage = apollo_protobuf::protobuf::StarkAuthentication;
    )
)]
pub trait Negotiator: Send + Clone {
    type WireMessage: Message + Default + Unpin + Send;
    type Error: std::error::Error + Send + Sync;

    /// Performs the handshake protocol.
    /// `connection_sender` is the channel that can be used to send data to the remote peer.
    /// `connection_receiver` is the channel that can be used to receive data from the remote peer.
    async fn negotiate_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection_sender: &mut dyn ConnectionSender<Self::WireMessage>,
        connection_receiver: &mut dyn ConnectionReceiver<Self::WireMessage>,
        side: NegotiationSide,
    ) -> Result<NegotiatorOutput, Self::Error>;

    /// A unique identified for your authentication protocol. For example: "strk_id" or
    /// "strk_id_v2".
    // TODO(noam.s): Consider making this a const.
    fn protocol_name(&self) -> &'static str;
}

// Automock does not implement Clone, so we need to do it manually.
// # Important: Clone Behavior
// **Warning**: Cloning a `MockNegotiator` returns a fresh mock with **no expectations set**.
// Any expectations configured on the original mock before cloning will be lost in the clone.
// This is a limitation of mockall - it intentionally doesn't preserve expectations across clones.
#[cfg(test)]
impl Clone for MockNegotiator {
    fn clone(&self) -> Self {
        MockNegotiator::default()
    }
}

/// This is a dummy implementation of the Negotiator trait used only so you can use
/// `Option<Negotiator>::None` (where you don't have a real type). Instances of this type should
/// never be created.
// We make it an enum to enforce that it is not possible to create an instance of this type.
// TODO(noam.s): Try to remove this.
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) enum DummyNegotiatorType {}

#[async_trait]
impl Negotiator for DummyNegotiatorType {
    type WireMessage = apollo_protobuf::protobuf::StarkAuthentication;
    type Error = IoError;

    fn protocol_name(&self) -> &'static str {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }

    async fn negotiate_connection(
        &mut self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection_sender: &mut dyn ConnectionSender<Self::WireMessage>,
        _connection_receiver: &mut dyn ConnectionReceiver<Self::WireMessage>,
        _side: NegotiationSide,
    ) -> Result<NegotiatorOutput, Self::Error> {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }
}
