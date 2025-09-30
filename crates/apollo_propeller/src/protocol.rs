//! Propeller protocol definitions and message handling.

use std::convert::Infallible;
use std::pin::Pin;

use asynchronous_codec::{Decoder, Encoder, Framed};
use bytes::BytesMut;
use futures::future;
use futures::prelude::*;
use libp2p::core::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::swarm::StreamProtocol;
use quick_protobuf_codec::Codec;

use crate::generated::propeller::pb as proto;
use crate::message::PropellerMessage;

/// Propeller protocol upgrade for libp2p streams.
#[derive(Debug, Clone)]
pub struct PropellerProtocol {
    protocol_id: StreamProtocol,
    max_shard_size: usize,
}

impl PropellerProtocol {
    /// Create a new Propeller protocol.
    pub fn new(protocol_id: StreamProtocol, max_shard_size: usize) -> Self {
        Self { protocol_id, max_shard_size }
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
        let codec = PropellerCodec::new(self.max_shard_size);
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
        let codec = PropellerCodec::new(self.max_shard_size);
        Box::pin(future::ok(Framed::new(socket, codec)))
    }
}

// Propeller codec for the framing

pub struct PropellerCodec {
    /// The codec to handle common encoding/decoding of protobuf messages
    codec: Codec<proto::PropellerMessage>,
}

impl PropellerCodec {
    pub fn new(max_shard_size: usize) -> PropellerCodec {
        let codec = Codec::new(max_shard_size);
        PropellerCodec { codec }
    }
}

impl Encoder for PropellerCodec {
    type Item<'a> = PropellerMessage;
    type Error = quick_protobuf_codec::Error;

    fn encode(&mut self, item: Self::Item<'_>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let proto_message: proto::PropellerMessage = item.into();
        self.codec.encode(proto_message, dst)
    }
}

impl Decoder for PropellerCodec {
    type Item = PropellerMessage;
    type Error = quick_protobuf_codec::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let src_len = src.len();

        let proto_message = match self.codec.decode(src) {
            Ok(Some(msg)) => msg,
            Ok(None) => return Ok(None),
            Err(e) => {
                return Err(e);
            }
        };

        // Convert from protobuf to our message type
        let message = match PropellerMessage::try_from(proto_message) {
            Ok(msg) => msg,
            Err(e) => {
                tracing::warn!(
                    "Failed to convert protobuf message: error={}, src_len={}",
                    e,
                    src_len
                );
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid protobuf message: {}", e),
                )
                .into());
            }
        };
        Ok(Some(message))
    }
}

#[cfg(test)]
mod tests {
    use asynchronous_codec::{Decoder, Encoder};
    use bytes::BytesMut;

    use super::*;
    use crate::message::PropellerMessage;

    #[test]
    fn test_propeller_codec_roundtrip() {
        let mut codec = PropellerCodec::new(65536);
        let mut buffer = BytesMut::new();

        let original_message = PropellerMessage::random(&mut rand::thread_rng(), 65536);

        // Encode
        codec.encode(original_message.clone(), &mut buffer).unwrap();

        // Decode
        let decoded_message = codec.decode(&mut buffer).unwrap().unwrap();

        // Verify
        assert_eq!(original_message, decoded_message);
    }
}
