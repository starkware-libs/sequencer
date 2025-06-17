use std::pin::Pin;

use async_trait::async_trait;
use futures::channel::mpsc::SendError;
use futures::{Sink, Stream};
use libp2p::PeerId;

pub type NegotiatorInitiatorResult = Result<(), NegotiatorInitiatorError>;
pub enum NegotiatorInitiatorError {
    FailedAuthentication,
}

/// Defines how to negotiate authentication as the initiator side.
#[async_trait]
pub trait NegotiatorInitiator {
    type MessageDeserializationError: Send + Sync;
    type SentMessage: TryFrom<Vec<u8>, Error = Self::MessageDeserializationError>
        + Into<Vec<u8>>
        + Send
        + Sync;
    type ReceivedMessage: TryFrom<Vec<u8>, Error = Self::MessageDeserializationError>
        + Into<Vec<u8>>
        + Send
        + Sync;

    /// Sends and receives the messages of the authentication protocol for the side which is
    /// starting the authentication protocol.
    // TODO(guy.f): Replace sender and receiver to using dyn Trait.
    async fn negotiate(
        &self,
        responder_peer_id: &PeerId,
        sender: Pin<Box<dyn Sink<Self::SentMessage, Error = SendError> + Send + 'async_trait>>,
        receiver: Pin<Box<dyn Stream<Item = Self::ReceivedMessage> + Send + 'async_trait>>,
    ) -> NegotiatorInitiatorResult
    where
        Self: Sync + Send + 'async_trait;
}

pub type NegotiatorResponderResult = Result<(), NegotiatorResponderError>;
pub enum NegotiatorResponderError {
    FailedAuthentication,
}
/// Defines how to negotiate authentication as the responder side.
#[async_trait]
pub trait NegotiatorResponder {
    type MessageDeserializationError: Send + Sync;
    type SentMessage: TryFrom<Vec<u8>, Error = Self::MessageDeserializationError>
        + Into<Vec<u8>>
        + Send
        + Sync;
    type ReceivedMessage: TryFrom<Vec<u8>, Error = Self::MessageDeserializationError>
        + Into<Vec<u8>>
        + Send
        + Sync;

    /// Sends and receives the messages of the authentication protocol for the side which is
    /// responding in the authentication protocol.
    // TODO(guy.f): Replace sender and receiver to using dyn Trait.
    async fn negotiate(
        &self,
        initiator_peer_id: &PeerId,
        sender: Pin<Box<dyn Sink<Self::SentMessage, Error = SendError> + Send + 'async_trait>>,
        receiver: Pin<Box<dyn Stream<Item = Self::ReceivedMessage> + Send + 'async_trait>>,
    ) -> NegotiatorResponderResult
    where
        Self: Sync + Send + 'async_trait;
}
