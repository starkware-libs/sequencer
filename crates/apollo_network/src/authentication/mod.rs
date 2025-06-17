use async_trait::async_trait;
use libp2p::PeerId;
use tokio::sync::mpsc::{Receiver, Sender};

pub struct SentMessage(pub Vec<u8>);

pub struct ReceivedMessage(pub Vec<u8>);

impl Deref for SentMessage {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for ReceivedMessage {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Defines how to negotiate authentication as the initiator side.
#[async_trait]
trait NegotiatorInitiator {
    /// Sends and receives the messages of the authentication protocol for the side which is
    /// starting the authentication protocol.
    async fn negotiate(
        &self,
        responder_peer_id: &PeerId,
        sender: Sender<SentMessage>,
        receiver: Receiver<ReceivedMessage>,
    ) -> Result<(), String>;
}

/// Defines how to negotiate authentication as the responder side.
#[async_trait]
trait NegotiatorResponder {
    /// Sends and receives the messages of the authentication protocol for the side which is
    /// responding in the authentication protocol.
    async fn negotiate(
        &self,
        initiator_peer_id: &PeerId,
        sender: Sender<SentMessage>,
        receiver: Receiver<ReceivedMessage>,
    ) -> Result<(), String>;
}
