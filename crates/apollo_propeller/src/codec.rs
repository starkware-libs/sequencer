use std::marker::PhantomData;

use asynchronous_codec::{Decoder, Encoder};
use bytes::{Buf, BytesMut};
use prost::Message;

#[derive(Debug, Clone)]
pub struct ProstCodec<T> {
    /// Maximum permitted number of bytes per message
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
        // Try to decode the length delimiter first
        let mut buf = src.as_ref();
        let original_len = buf.len();

        // Decode just the length prefix to check if we have enough data
        let length = match prost::decode_length_delimiter(&mut buf) {
            Ok(len) => len,
            Err(_) => return Ok(None), // Not enough data for length prefix
        };

        // Check if message exceeds maximum
        if length > self.max_message_len_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "message length exceeds maximum",
            ));
        }

        let delimiter_len = original_len - buf.len();
        let total_len = delimiter_len + length;

        // Check if we have the complete message
        if src.len() < total_len {
            src.reserve(total_len - src.len());
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
                "message length exceeds maximum when encoding",
            ));
        }

        let delimiter_len = prost::length_delimiter_len(msg_len);
        dst.reserve(delimiter_len + msg_len);

        item.encode_length_delimited(dst)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(())
    }
}
