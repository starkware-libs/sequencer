use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lazy_static::lazy_static;

lazy_static! {
    /// Size of the serialized header preceding the payload. Computed from an empty message
    /// so the size is correct even if the header format changes.
    pub static ref METADATA_SIZE: usize = {
        let empty_message = StressTestMessage::new(0, 0, vec![]);
        let serialized: Vec<u8> = empty_message.into();
        serialized.len()
    };
}

#[derive(Debug, Clone, Copy)]
pub struct StressTestMessageMetadata {
    pub sender_id: u64,
    pub message_index: u64,
    pub time: SystemTime,
}

#[derive(Debug, Clone)]
pub struct StressTestMessage {
    pub metadata: StressTestMessageMetadata,
    pub payload: Vec<u8>,
}

/// Error returned when a `StressTestMessage` cannot be parsed from bytes received on the wire.
#[derive(Debug)]
pub struct StressTestMessageParseError(pub String);

impl std::fmt::Display for StressTestMessageParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to parse StressTestMessage: {}", self.0)
    }
}

impl std::error::Error for StressTestMessageParseError {}

impl StressTestMessage {
    pub fn new(sender_id: u64, message_index: u64, payload: Vec<u8>) -> Self {
        StressTestMessage {
            metadata: StressTestMessageMetadata {
                sender_id,
                message_index,
                time: SystemTime::now(),
            },
            payload,
        }
    }

    #[cfg(test)]
    pub fn slow_len(self) -> usize {
        let seq = Vec::<u8>::from(self);
        seq.len()
    }

    pub fn len(&self) -> usize {
        *METADATA_SIZE + self.payload.len()
    }
}

impl From<StressTestMessage> for Vec<u8> {
    fn from(value: StressTestMessage) -> Self {
        let payload_len = u64::try_from(value.payload.len())
            .expect("usize fits in u64 on all supported platforms");
        let duration = value
            .metadata
            .time
            .duration_since(UNIX_EPOCH)
            .expect("message was constructed after UNIX_EPOCH");
        [
            &value.metadata.sender_id.to_be_bytes()[..],
            &value.metadata.message_index.to_be_bytes()[..],
            &duration.as_secs().to_be_bytes()[..],
            &duration.subsec_nanos().to_be_bytes()[..],
            &payload_len.to_be_bytes()[..],
            &value.payload[..],
        ]
        .concat()
    }
}

impl TryFrom<Vec<u8>> for StressTestMessage {
    type Error = StressTestMessageParseError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        let mut offset = 0;

        let mut take = |num_bytes: usize| -> Result<&[u8], StressTestMessageParseError> {
            let end = offset + num_bytes;
            if end > bytes.len() {
                return Err(StressTestMessageParseError(format!(
                    "truncated at offset {offset}: need {num_bytes} bytes, have {}",
                    bytes.len() - offset,
                )));
            }
            let slice = &bytes[offset..end];
            offset = end;
            Ok(slice)
        };

        let sender_id =
            u64::from_be_bytes(take(8)?.try_into().expect("take(8) returns exactly 8 bytes"));
        let message_index =
            u64::from_be_bytes(take(8)?.try_into().expect("take(8) returns exactly 8 bytes"));
        let secs =
            u64::from_be_bytes(take(8)?.try_into().expect("take(8) returns exactly 8 bytes"));
        let nanos =
            u32::from_be_bytes(take(4)?.try_into().expect("take(4) returns exactly 4 bytes"));
        let time = UNIX_EPOCH + Duration::new(secs, nanos);

        let payload_len_u64 =
            u64::from_be_bytes(take(8)?.try_into().expect("take(8) returns exactly 8 bytes"));
        let payload_len = usize::try_from(payload_len_u64).map_err(|_| {
            StressTestMessageParseError(format!("payload_len {payload_len_u64} exceeds usize"))
        })?;
        let payload = take(payload_len)?.to_vec();

        Ok(StressTestMessage {
            metadata: StressTestMessageMetadata { sender_id, message_index, time },
            payload,
        })
    }
}
