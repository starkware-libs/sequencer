use async_trait::async_trait;
use libp2p::PeerId;
use tokio::sync::mpsc::{Receiver, Sender};

pub enum NegotiationResult {
    Success,
    Failure,
}

pub struct SentMessage(Vec<u8>);

pub struct ReceivedMessage(Vec<u8>);

/// Defines how to negotiate authentication as the initiator side.
#[async_trait]
trait NegotiatorInitiator {
    /// Sends and receives the messages of the authentication protocol for the side which is
    /// starting the authentication protocol.
    /// `peer_id` is the ID of the peer we are negotiating with.
    async fn negotiate(
        &self,
        peer_id: &PeerId,
        sender: Sender<SentMessage>,
        receiver: Receiver<ReceivedMessage>,
    ) -> NegotiationResult;
}

/// Defines how to negotiate authentication as the responder side.
#[async_trait]
trait NegotiatorResponder {
    /// Sends and receives the messages of the authentication protocol for the side which is
    /// responding in the authentication protocol.
    /// `peer_id` is the ID of the peer we are negotiating with.
    async fn negotiate(
        &self,
        peer_id: &PeerId,
        sender: Sender<SentMessage>,
        receiver: Receiver<ReceivedMessage>,
    ) -> NegotiationResult;
}
