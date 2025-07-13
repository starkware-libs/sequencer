use std::convert::TryFrom;

use async_trait::async_trait;
use futures::{Sink, Stream};
use libp2p::noise::Error;
use libp2p::PeerId;

pub enum NegotiatorResult {
    Ok,
    /// Returned when the handshake concluded that the currently connecting peer is a duplicate of
    /// the currently connected peer. `PeerId` is the (older) currently connected peer.
    DuplicatePeer(PeerId),
}

#[async_trait]
pub trait Negotiator: Send + Clone {
    type Message: TryFrom<Vec<u8>> + Into<Vec<u8>> + Send;

    async fn negotiate_incoming_connection<NegotiatorChannel>(
        &self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatorResult, Error>
    where
        NegotiatorChannel: Sink<Self::Message, Error = std::io::Error>
            + Stream<Item = Result<Self::Message, std::io::Error>>
            + Unpin
            + Send;

    async fn negotiate_outgoing_connection<NegotiatorChannel>(
        &self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatorResult, Error>
    where
        NegotiatorChannel: Sink<Self::Message, Error = std::io::Error>
            + Stream<Item = Result<Self::Message, std::io::Error>>
            + Unpin
            + Send;

    /// For example: "strk_id" or "strk_id_v2".
    fn protocol_name(&self) -> &'static str;
}
