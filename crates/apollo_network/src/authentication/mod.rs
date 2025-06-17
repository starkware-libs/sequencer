use std::convert::Infallible;
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
    type MessageDeserializationError;
    type SentMessage: TryFrom<Vec<u8>, Error = Self::MessageDeserializationError> + Into<Vec<u8>>;
    type ReceivedMessage: TryFrom<Vec<u8>, Error = Self::MessageDeserializationError>
        + Into<Vec<u8>>;

    /// Sends and receives the messages of the authentication protocol for the side which is
    /// starting the authentication protocol.
    // TODO(guy.f): Replace sender and receiver to using dyn Trait.
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
#[async_trait]
pub trait NegotiatorResponder {
    type MessageDeserializationError;
    type SentMessage: TryFrom<Vec<u8>, Error = Self::MessageDeserializationError> + Into<Vec<u8>>;
    type ReceivedMessage: TryFrom<Vec<u8>, Error = Self::MessageDeserializationError>
        + Into<Vec<u8>>;

    /// Sends and receives the messages of the authentication protocol for the side which is
    /// responding in the authentication protocol.
    // TODO(guy.f): Replace sender and receiver to using dyn Trait.
    async fn negotiate(
        &self,
        initiator_peer_id: &PeerId,
        sender: DynSender<Self::SentMessage>,
        receiver: DynReceiver<Self::ReceivedMessage>,
    ) -> NegotiatorResponderResult;
}

type DynSender<M> = Pin<Box<dyn Sink<M, Error = SendError> + Send>>;
type DynReceiver<M> = Pin<Box<dyn Stream<Item = M> + Send>>;

// TODO: Remove this before submitting. Keeping for now as a code example and to make sure code
// compiles.

struct Foo();

#[async_trait]
impl NegotiatorResponder for Foo {
    type MessageDeserializationError = Infallible;
    type SentMessage = Vec<u8>;
    type ReceivedMessage = Vec<u8>;

    async fn negotiate(
        &self,
        _responder_peer_id: &PeerId,
        _sender: DynSender<Self::SentMessage>,
        _receiver: DynReceiver<Self::ReceivedMessage>,
    ) -> NegotiatorResponderResult {
        Ok(())
    }
}
