use papyrus_test_utils::{get_rng, GetTestInstance};
use rand::Rng;

use crate::consensus::{ConsensusMessage, Proposal, StreamMessage};

// TODO(guyn): add tests for other serializable objects in consensus

impl GetTestInstance for StreamMessage<ConsensusMessage> {
    fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
        Self {
            message: ConsensusMessage::Proposal(Proposal::default()),
            stream_id: rng.gen_range(0..100),
            chunk_id: rng.gen_range(0..1000),
            fin: rng.gen_bool(0.5),
        }
    }
}

#[test]
fn convert_stream_message_to_vec_u8_and_back() {
    let mut rng = get_rng();

    // test that we can convert a StreamMessage with a ConsensusMessage message to bytes and back
    let stream_message: StreamMessage<ConsensusMessage> =
        StreamMessage::get_test_instance(&mut rng);

    let bytes_data: Vec<u8> = stream_message.clone().into();
    let res_data = StreamMessage::try_from(bytes_data).unwrap();
    assert_eq!(stream_message, res_data);
}
