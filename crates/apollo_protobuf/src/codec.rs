//! Length-delimited protobuf codec for asynchronous streaming.
//!
//! This module provides a [`ProstCodec`] implementation that encodes and decodes
//! Protocol Buffer messages with length delimiters, suitable for use with stream-based
//! protocols like TCP where message boundaries need to be preserved.
//!
//! The codec uses varint-encoded length prefixes (via prost's length-delimited format)
//! and enforces a configurable maximum message size to prevent resource exhaustion.

use std::marker::PhantomData;

use asynchronous_codec::{Decoder, Encoder};
use bytes::{Buf, BytesMut};
use prost::Message;

#[derive(Debug, Clone)]
pub struct ProstCodec<T> {
    /// Maximum permitted number of bytes per message, prevents resource attacks
    max_message_len_bytes: usize,
    /// Needed for setting the generic type parameter
    _marker: PhantomData<T>,
}

impl<T> ProstCodec<T> {
    pub fn new(max_message_len_bytes: usize) -> Self {
        Self { max_message_len_bytes, _marker: PhantomData }
    }
}

impl<T: Message + Default> Decoder for ProstCodec<T> {
    type Item = T;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let (length, remaining) = match unsigned_varint::decode::usize(src) {
            Ok(result) => result,
            Err(unsigned_varint::decode::Error::Insufficient) => {
                // Not enough bytes to decode the complete varint yet
                return Ok(None);
            }
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid varint in length delimiter: {}", e),
                ));
            }
        };
        let length_bytes = src.len() - remaining.len();

        // Check if message exceeds maximum
        if length > self.max_message_len_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "message length exceeds maximum",
            ));
        }

        let total_len = length_bytes + length;

        // Check if we have the complete message
        if src.len() < total_len {
            // Not reserving here to avoid performance attacks
            return Ok(None);
        }

        // We have enough data, decode the message
        let message = T::decode_length_delimited(&mut src.as_ref())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Advance the buffer past the consumed bytes
        src.advance(total_len);

        Ok(Some(message))
    }
}

impl<T: Message> Encoder for ProstCodec<T> {
    type Item<'a> = T;
    type Error = std::io::Error;

    fn encode(&mut self, item: Self::Item<'_>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg_len = item.encoded_len();

        if msg_len > self.max_message_len_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "message length ({msg_len}) exceeds maximum ({}) when encoding",
                    self.max_message_len_bytes
                ),
            ));
        }

        let delimiter_len = prost::length_delimiter_len(msg_len);
        dst.reserve(delimiter_len + msg_len);
        item.encode_length_delimited(dst)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(())
    }
}
