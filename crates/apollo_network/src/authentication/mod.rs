use async_trait::async_trait;
use futures::io::Sink;
use futures::{SinkExt, StreamExt};
use libp2p::PeerId;

pub struct MessageDeserializationError();

pub type NegotiatorInitiatorResult = Result<(), NegotiatorInitiatorError>;
pub enum NegotiatorInitiatorError {
    FAILED_AUTHENTICATION,
}

/// Defines how to negotiate authentication as the initiator side.
#[async_trait]
trait NegotiatorInitiator {
    type SentMessage: TryFrom<Vec<u8>, Error = MessageDeserializationError>
        + Into<Vec<u8>>
        + Send
        + Sync;
    type ReceivedMessage: TryFrom<Vec<u8>, Error = MessageDeserializationError>
        + Into<Vec<u8>>
        + Send
        + Sync;

    /// Sends and receives the messages of the authentication protocol for the side which is
    /// starting the authentication protocol.
    // TODO(guy.f): Replace sender and receiver to using dyn Trait.
    async fn negotiate(
        &self,
        responder_peer_id: &PeerId,
        sender: SinkExt<Self::SentMessage>,
        receiver: StreamExt<Self::ReceivedMessage>,
    ) -> NegotiatorInitiatorResult;
}

pub type NegotiatorResponderResult = Result<(), NegotiatorResponderError>;
pub enum NegotiatorResponderError {
    FAILED_AUTHENTICATION,
}
/// Defines how to negotiate authentication as the responder side.
#[async_trait]
trait NegotiatorResponder {
    type Sent: TryFrom<Vec<u8>, Error = MessageDeserializationError> + Into<Vec<u8>> + Send + Sync;
    type Received: TryFrom<Vec<u8>, Error = MessageDeserializationError>
        + Into<Vec<u8>>
        + Send
        + Sync;

    /// Sends and receives the messages of the authentication protocol for the side which is
    /// responding in the authentication protocol.
    // TODO(guy.f): Replace sender and receiver to using dyn Trait.
    async fn negotiate(
        &self,
        initiator_peer_id: &PeerId,
        sender: SinkExt<Self::SentMessage>,
        receiver: StreamExt<Self::ReceivedMessage>,
    ) -> NegotiatorResponderResult;
}
