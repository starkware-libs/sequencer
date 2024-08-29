use papyrus_test_utils::{get_rng, auto_impl_get_test_instance, GetTestInstance};
use crate::consensus::StreamMessage;

auto_impl_get_test_instance!{
    pub struct StreamMessage {
        message: Vec<u8>,
        stream_id: u64,
        chunk_id: u64,
        fin: bool,
    }
}


#[test]
fn convert_stream_message_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let stream_message = StreamMessage::get_test_instance(&mut rng);

    let bytes_data = Vec::<u8>::from(stream_message.clone());
    let res_data = StreamMessage::try_from(bytes_data).unwrap();
    assert_eq!(stream_message, res_data);
}