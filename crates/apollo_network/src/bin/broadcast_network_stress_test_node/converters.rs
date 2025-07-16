use std::mem::size_of;
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use libp2p::PeerId;

pub const METADATA_SIZE: usize = size_of::<u32>() + size_of::<u64>() + size_of::<u32>() + 38;

#[derive(Debug, Clone)]
pub struct StressTestMessage {
    pub id: u32,
    pub payload: Vec<u8>,
    pub time: SystemTime,
    pub peer_id: String,
}

impl StressTestMessage {
    pub fn new(id: u32, payload: Vec<u8>, peer_id: String) -> Self {
        StressTestMessage { id, payload, time: SystemTime::now(), peer_id }
    }
}

impl From<StressTestMessage> for Vec<u8> {
    fn from(value: StressTestMessage) -> Self {
        let StressTestMessage { id, mut payload, time, peer_id } = value;
        let id = id.to_be_bytes().to_vec();
        let time = time.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let seconds = time.as_secs().to_be_bytes().to_vec();
        let nanos = time.subsec_nanos().to_be_bytes().to_vec();
        let peer_id = PeerId::from_str(&peer_id).unwrap().to_bytes();
        payload.extend(id);
        payload.extend(seconds);
        payload.extend(nanos);
        payload.extend(peer_id);
        payload
    }
}

impl From<Vec<u8>> for StressTestMessage {
    // This auto implements TryFrom<Vec<u8>> for StressTestMessage
    fn from(mut value: Vec<u8>) -> Self {
        let vec_size = value.len();
        let payload_size = vec_size - METADATA_SIZE;
        let id_and_time = value.split_off(payload_size);
        let id = u32::from_be_bytes(id_and_time[0..4].try_into().unwrap());
        let seconds = u64::from_be_bytes(id_and_time[4..12].try_into().unwrap());
        let nanos = u32::from_be_bytes(id_and_time[12..16].try_into().unwrap());
        let time = SystemTime::UNIX_EPOCH + Duration::new(seconds, nanos);
        let peer_id = PeerId::from_bytes(&id_and_time[16..]).unwrap().to_string();
        StressTestMessage { id, payload: value, time, peer_id }
    }
}
