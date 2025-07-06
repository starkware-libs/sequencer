use std::fmt::Debug;

use async_trait::async_trait;
use futures::{Sink, SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Bytes = Vec<u8>;

#[derive(
    Clone, Debug, Default, derive_more::Deref, Eq, PartialEq, Hash, Serialize, Deserialize,
)]
pub struct PeerId(pub Vec<u8>);

impl From<Vec<u8>> for PeerId {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

#[async_trait]
pub trait AuthNegotiator {
    type Error: Debug;

    /// Performs a one-sided authentication protocol.
    async fn negotiate<S, R>(
        &self,
        other_peer_id: PeerId,
        connection: &mut ConnectionEnd<S, R>,
    ) -> Result<bool, Self::Error>
    where
        S: Sink<Bytes> + Unpin + Send,
        R: Stream<Item = Bytes> + Unpin + Send;

    fn peer_id(&self) -> PeerId;
}

pub struct AuthProtocol<I, R> {
    pub initiator: I,
    pub responder: R,
}

impl<I, R> AuthProtocol<I, R>
where
    I: AuthNegotiator,
    R: AuthNegotiator,
{
    pub fn new(initiator: I, responder: R) -> Self {
        Self { initiator, responder }
    }

    pub async fn negotiate<Connection>(
        &self,
        connection: Connection,
    ) -> (Result<bool, I::Error>, Result<bool, R::Error>)
    where
        Connection: BidirectionalConnection,
        Connection::S: Sink<Bytes> + Unpin + Send,
        Connection::R: Stream<Item = Bytes> + Unpin + Send,
    {
        let [mut initiator_end, mut responder_end] = connection.split();

        let initiator_task = self.initiator.negotiate(self.responder.peer_id(), &mut initiator_end);
        let responder_task = self.responder.negotiate(self.initiator.peer_id(), &mut responder_end);

        tokio::join!(initiator_task, responder_task)
    }
}

pub trait BidirectionalConnection {
    type S: Sink<Bytes> + Unpin + Send;
    type R: Stream<Item = Bytes> + Unpin + Send;

    /// Split into two ends - each end has both a stream and sink
    fn split(self) -> [ConnectionEnd<Self::S, Self::R>; 2];
}

pub struct ConnectionEnd<S, R> {
    pub sender: S,
    pub receiver: R,
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("Failed to receive message.")]
    Recv,
    #[error("Failed to send message.")]
    Send,
}

impl<S, R> ConnectionEnd<S, R>
where
    S: Sink<Bytes> + Unpin,
    R: Stream<Item = Bytes> + Unpin,
{
    pub async fn send(&mut self, message: Bytes) -> Result<(), ConnectionError> {
        self.sender.send(message).await.map_err(|_| ConnectionError::Send)
    }

    pub async fn recv(&mut self) -> Result<Bytes, ConnectionError> {
        self.receiver.next().await.ok_or(ConnectionError::Recv)
    }
}
