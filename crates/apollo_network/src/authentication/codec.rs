use asynchronous_codec::{Decoder, Encoder};
use bytes::{Buf, BytesMut};

#[derive(Default)]
pub(crate) struct NegotiatorChannelCodec;

// This encoder works by writing the size of the message first (as a varint), followed by the
// message itself serialized to bytes.
// TODO(noam.s): Remove shared code with sqmr/messages.rs.
impl Encoder for NegotiatorChannelCodec {
    type Item<'a> = Vec<u8>;
    type Error = std::io::Error;

    fn encode(&mut self, item: Self::Item<'_>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Encode the size using unsigned_varint
        let mut size_buf = unsigned_varint::encode::usize_buffer();
        let size_bytes = unsigned_varint::encode::usize(item.len(), &mut size_buf);

        // Write the size followed by the data
        dst.extend_from_slice(size_bytes);
        dst.extend_from_slice(&item);

        Ok(())
    }
}

// Decodes a message encoded with the NegotiatorChannelCodec. See there for more details.
impl Decoder for NegotiatorChannelCodec {
    type Item = Vec<u8>;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Try to decode the size first
        let original_len = src.len();
        let (size, remaining_bytes) = match unsigned_varint::decode::usize(src) {
            Ok((size, remaining)) => (size, remaining),
            Err(unsigned_varint::decode::Error::Insufficient) => {
                // Not enough bytes to decode the size yet
                return Ok(None);
            }
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to decode size: {e}"),
                ));
            }
        };

        let size_bytes_consumed = original_len - remaining_bytes.len();

        // Check if we have enough bytes for the complete message
        if src.len() < size_bytes_consumed + size {
            // Not enough bytes yet
            return Ok(None);
        }

        // Remove the size bytes from the buffer
        src.advance(size_bytes_consumed);

        // Extract the data bytes
        let data = src.split_to(size).to_vec();

        Ok(Some(data))
    }
}
