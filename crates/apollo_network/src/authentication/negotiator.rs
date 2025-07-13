use async_trait::async_trait;
use futures::{Sink, Stream};
use libp2p::PeerId;

pub enum NegotiatonSuccessData {
    None,
    /// Returned when the handshake concluded that the currently connecting peer is a duplicate of
    /// the currently connected peer. `PeerId` is the (older) currently connected peer.
    DuplicatePeer(PeerId),
}

#[async_trait]
pub trait Negotiator: Send + Clone {
    /// Performs the handshake protocol when we are the incoming connection side.
    /// `connection` is the channel that can be used to communicate with the other peer.
    async fn negotiate_incoming_connection<NegotiatorChannel>(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatonSuccessData, crate::authentication::Error>
    where
        NegotiatorChannel: Sink<Vec<u8>, Error = std::io::Error>
            + Stream<Item = Result<Vec<u8>, std::io::Error>>
            + Unpin
            + Send;

    /// Performs the handshake protocol when we are the outgoing connection side.
    /// `connection` is the channel that can be used to communicate with the other peer.
    async fn negotiate_outgoing_connection<NegotiatorChannel>(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatonSuccessData, crate::authentication::Error>
    where
        NegotiatorChannel: Sink<Vec<u8>, Error = std::io::Error>
            + Stream<Item = Result<Vec<u8>, std::io::Error>>
            + Unpin
            + Send;

    /// A unique identified for your authentication protocol. For example: "strk_id" or
    /// "strk_id_v2".
    fn protocol_name(&self) -> &'static str;
}
