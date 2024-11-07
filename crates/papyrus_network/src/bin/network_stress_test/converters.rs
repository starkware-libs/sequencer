use std::time::{Duration, SystemTime};

struct StressTestPayload(u32, Vec<u8>, SystemTime);

impl From<StressTestPayload> for Vec<u8> {
    fn from(value: StressTestPayload) -> Self {
        let StressTestPayload(id, payload, time) = value;
        let id = id.to_be_bytes().to_vec();
        let time = time.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let seconds = time.as_secs().to_be_bytes().to_vec();
        let nanos = time.subsec_nanos().to_be_bytes().to_vec();
        [id, seconds, nanos, payload].concat()
    }
}

impl From<Vec<u8>> for StressTestPayload {
    fn from(value: Vec<u8>) -> Self {
        let id = u32::from_be_bytes([value[0], value[1], value[2], value[3]]);
        let seconds = u64::from_be_bytes([
            value[4], value[5], value[6], value[7], value[8], value[9], value[10], value[11],
        ]);
        let nanos = u32::from_be_bytes([value[12], value[13], value[14], value[15]]);
        let time = SystemTime::UNIX_EPOCH + Duration::new(seconds, nanos);
        let payload = value[16..].to_vec();
        StressTestPayload(id, payload, time)
    }
}
