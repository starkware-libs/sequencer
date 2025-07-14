use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lazy_static::lazy_static;

lazy_static! {
    // Calculate actual metadata size based on serialized empty message
    pub static ref METADATA_SIZE: usize = {
        let empty_message = StressTestMessage::new(0, 0, vec![]);
        let serialized: Vec<u8> = empty_message.into();
        serialized.len()
    };
}

#[derive(Debug, Clone, Copy)]
pub struct StressTestMessageMetaData {
    pub sender_id: u64,
    pub message_index: u64,
    pub time: SystemTime,
}

#[derive(Debug, Clone)]
pub struct StressTestMessage {
    pub metadata: StressTestMessageMetaData,
    pub payload: Vec<u8>,
}

impl StressTestMessage {
    pub fn new(sender_id: u64, message_index: u64, payload: Vec<u8>) -> Self {
        StressTestMessage {
            metadata: StressTestMessageMetaData {
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
        let payload_len: u64 = value.payload.len().try_into().unwrap();
        [
            &value.metadata.sender_id.to_be_bytes()[..],
            &value.metadata.message_index.to_be_bytes()[..],
            &value.metadata.time.duration_since(UNIX_EPOCH).unwrap().as_nanos().to_be_bytes()[..],
            &payload_len.to_be_bytes()[..],
            &value.payload[..],
        ]
        .concat()
    }
}

impl From<Vec<u8>> for StressTestMessage {
    fn from(bytes: Vec<u8>) -> Self {
        let mut i = 0;
        let mut get = |n: usize| {
            let r = &bytes[i..i + n];
            i += n;
            r
        };

        let sender_id = u64::from_be_bytes(get(8).try_into().unwrap());
        let message_index = u64::from_be_bytes(get(8).try_into().unwrap());
        let time = UNIX_EPOCH
            + Duration::from_nanos(
                u128::from_be_bytes(get(16).try_into().unwrap()).try_into().unwrap(),
            );
        let payload_len = u64::from_be_bytes(get(8).try_into().unwrap()).try_into().unwrap();
        let payload = get(payload_len).to_vec();

        StressTestMessage {
            metadata: StressTestMessageMetaData { sender_id, message_index, time },
            payload,
        }
    }
}
