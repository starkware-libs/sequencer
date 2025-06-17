use async_trait::async_trait;
use libp2p::PeerId;
use tokio::sync::mpsc::{Receiver, Sender};

pub struct SentMessage<T>(pub T);

pub struct ReceivedMessage<T>(pub T);

impl<T> Deref for SentMessage<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Deref for ReceivedMessage<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub type NegotiatorInitiatorResult = Result<(), NegotiatorInitiatorError>;
pub enum NegotiatorInitiatorError {
    FAILED_AUTHENTICATION,
}

/// Defines how to negotiate authentication as the initiator side.
#[async_trait]
trait NegotiatorInitiator {
    type Sent: From<Vec<u8>> + Into<Vec<u8>> + Send + Sync;
    type Received: From<Vec<u8>> + Into<Vec<u8>> + Send + Sync;

    /// Sends and receives the messages of the authentication protocol for the side which is
    /// starting the authentication protocol.
    async fn negotiate(
        &self,
        responder_peer_id: &PeerId,
        sender: Sender<SentMessage<Self::Sent>>,
        receiver: Receiver<ReceivedMessage<Self::Received>>,
    ) -> NegotiatorInitiatorResult
}

pub type NegotiatorResponderResult = Result<(), NegotiatorResponderError>;
pub enum NegotiatorResponderError {
    FAILED_AUTHENTICATION,
}
/// Defines how to negotiate authentication as the responder side.
#[async_trait]
trait NegotiatorResponder {
    type Sent: From<Vec<u8>> + Into<Vec<u8>> + Send + Sync;
    type Received: From<Vec<u8>> + Into<Vec<u8>> + Send + Sync;

    /// Sends and receives the messages of the authentication protocol for the side which is
    /// responding in the authentication protocol.
    async fn negotiate(
        &self,
        initiator_peer_id: &PeerId,
        sender: Sender<SentMessage<Self::Sent>>,
        receiver: Receiver<ReceivedMessage<Self::Received>>,
    ) -> NegotiatorResponderResult;
}
