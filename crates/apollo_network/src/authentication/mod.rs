use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::mpsc::SendError;
use futures::{Sink, Stream};
use libp2p::PeerId;

pub type NegotiatorInitiatorResult = Result<(), NegotiatorInitiatorError>;
pub enum NegotiatorInitiatorError {
    FailedAuthentication,
}

/// Defines how to negotiate authentication as the initiator side.
// TODO(guy.f): Add doc string with example implementation.
#[async_trait]
pub trait NegotiatorInitiator {
    type MessageDeserializationError;
    type SentMessage: Into<Vec<u8>> + Into<Vec<u8>>;
    type ReceivedMessage: TryFrom<Vec<u8>, Error = Self::MessageDeserializationError>
        + Into<Vec<u8>>;

    /// Sends and receives the messages of the authentication protocol for the side which is
    /// starting the authentication protocol.
    async fn negotiate(
        &self,
        responder_peer_id: &PeerId,
        sender: DynSender<Self::SentMessage>,
        receiver: DynReceiver<Self::ReceivedMessage>,
    ) -> NegotiatorInitiatorResult;
}

pub type NegotiatorResponderResult = Result<(), NegotiatorResponderError>;
pub enum NegotiatorResponderError {
    FailedAuthentication,
}
/// Defines how to negotiate authentication as the responder side.
// TODO(guy.f): Add doc string with example implementation.
#[async_trait]
pub trait NegotiatorResponder {
    type MessageDeserializationError;
    type SentMessage: Into<Vec<u8>> + Into<Vec<u8>>;
    type ReceivedMessage: TryFrom<Vec<u8>, Error = Self::MessageDeserializationError>
        + Into<Vec<u8>>;

    /// Sends and receives the messages of the authentication protocol for the side which is
    /// responding in the authentication protocol.
    async fn negotiate(
        &self,
        initiator_peer_id: &PeerId,
        sender: DynSender<Self::SentMessage>,
        receiver: DynReceiver<Self::ReceivedMessage>,
    ) -> NegotiatorResponderResult;
}

type DynSender<M> = Pin<Arc<dyn Sink<M, Error = SendError> + Send + Sync>>;
type DynReceiver<M> = Pin<Arc<dyn Stream<Item = M> + Send + Sync>>;
