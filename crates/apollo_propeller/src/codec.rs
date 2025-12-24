//! Prost-based codec for propeller protocol messages.

use std::io::{self, Cursor};
use std::marker::PhantomData;

use asynchronous_codec::{Decoder, Encoder};
use bytes::{Buf, BytesMut};
use prost::Message;

/// A codec that encodes/decodes prost messages with length-prefix framing.
#[derive(Debug)]
pub struct ProstCodec<T> {
    max_message_size: usize,
    _marker: PhantomData<T>,
}

impl<T> ProstCodec<T> {
    /// Creates a new codec with the specified maximum message size.
    pub fn new(max_message_size: usize) -> Self {
        Self { max_message_size, _marker: PhantomData }
    }
}

impl<T> Clone for ProstCodec<T> {
    fn clone(&self) -> Self {
        Self { max_message_size: self.max_message_size, _marker: PhantomData }
    }
}

impl<T: Message + Default> Decoder for ProstCodec<T> {
    type Item = T;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        // Read the length prefix using prost's varint decoding
        let mut cursor = Cursor::new(&src[..]);
        let len = match prost::decode_length_delimiter(&mut cursor) {
            Ok(len) => len,
            Err(_) => {
                // Not enough bytes to read the length prefix
                return Ok(None);
            }
        };

        // Check if the message is too large
        if len > self.max_message_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Message too large: {len} > {}", self.max_message_size),
            ));
        }

        let prefix_len = cursor.position().try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Length prefix position overflow")
        })?;
        let total_len = prefix_len + len;

        // Check if we have enough bytes for the full message
        if src.len() < total_len {
            // Reserve space for the remaining bytes if needed
            src.reserve(total_len - src.len());
            return Ok(None);
        }

        // Skip the length prefix
        src.advance(prefix_len);

        // Decode the message
        let message_bytes = src.split_to(len);
        let message = T::decode(&message_bytes[..])
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(Some(message))
    }
}

impl<T: Message> Encoder for ProstCodec<T> {
    type Item<'a> = T;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item<'_>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg_len = item.encoded_len();

        if msg_len > self.max_message_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Message too large: {msg_len} > {}", self.max_message_size),
            ));
        }

        // Reserve space for the length prefix and message
        let prefix_len = prost::length_delimiter_len(msg_len);
        dst.reserve(prefix_len + msg_len);

        // Encode the length prefix directly to BytesMut (which implements BufMut)
        prost::encode_length_delimiter(msg_len, dst).map_err(io::Error::other)?;

        // Encode the message directly to BytesMut
        item.encode(dst).map_err(io::Error::other)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use apollo_protobuf::protobuf::PropellerUnitBatch;
    use bytes::BytesMut;

    use super::*;

    #[test]
    fn test_roundtrip_empty_batch() {
        let mut codec: ProstCodec<PropellerUnitBatch> = ProstCodec::new(1024);
        let original = PropellerUnitBatch { batch: vec![] };

        let mut buf = BytesMut::new();
        codec.encode(original.clone(), &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }
}
