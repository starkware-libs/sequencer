//! Propeller protocol definitions and message handling.

use std::convert::Infallible;
use std::pin::Pin;

use apollo_protobuf::protobuf::PropellerUnitBatch;
use asynchronous_codec::Framed;
use futures::future;
use futures::prelude::*;
use libp2p::core::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::swarm::StreamProtocol;

use crate::codec::ProstCodec;

/// Codec for the Propeller protocol.
pub type PropellerCodec = ProstCodec<PropellerUnitBatch>;

/// Propeller protocol upgrade for libp2p streams.
#[derive(Debug, Clone)]
pub struct PropellerProtocol {
    protocol_id: StreamProtocol,
    max_wire_message_size: usize,
}

impl PropellerProtocol {
    /// Create a new Propeller protocol.
    pub fn new(protocol_id: StreamProtocol, max_wire_message_size: usize) -> Self {
        Self { protocol_id, max_wire_message_size }
    }
}

impl UpgradeInfo for PropellerProtocol {
    type Info = StreamProtocol;
    type InfoIter = std::iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once(self.protocol_id.clone())
    }
}

impl<TSocket> InboundUpgrade<TSocket> for PropellerProtocol
where
    TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = Framed<TSocket, PropellerCodec>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
        let codec = PropellerCodec::new(self.max_wire_message_size);
        Box::pin(future::ok(Framed::new(socket, codec)))
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for PropellerProtocol
where
    TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = Framed<TSocket, PropellerCodec>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
        let codec = PropellerCodec::new(self.max_wire_message_size);
        Box::pin(future::ok(Framed::new(socket, codec)))
    }
}
