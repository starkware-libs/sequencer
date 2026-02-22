use std::io::Error as IoError;

use async_trait::async_trait;
use libp2p::PeerId;
use prost::Message;

pub enum NegotiationSide {
    Inbound,
    Outbound,
}

pub enum NegotiatorOutput {
    Success,
    /// Returned when the handshake concluded that the currently connecting peer is a duplicate of
    /// the currently connected peer. `PeerId` is the (older) currently connected peer.
    DuplicatePeer(PeerId),
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
