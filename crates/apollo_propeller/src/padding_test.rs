use rstest::*;

use crate::padding::{pad_message, unpad_message};
use crate::types::ReconstructionError;

fn encode_length(len: usize) -> Vec<u8> {
    let mut buf = unsigned_varint::encode::usize_buffer();
    unsigned_varint::encode::usize(len, &mut buf).to_vec()
}

fn make_message(claimed_len: usize, actual_data: &[u8]) -> Vec<u8> {
    [&encode_length(claimed_len)[..], actual_data].concat()
}

#[rstest]
#[case(vec![1, 2, 3], 4)]
#[case(vec![1, 2, 3, 4, 5], 6)]
#[case(vec![42; 100], 20)]
#[case(vec![], 10)]
fn test_pad_unpad_roundtrip(#[case] message: Vec<u8>, #[case] divisor: usize) {
    let padded = pad_message(message.clone(), divisor);
    assert_eq!(padded.len() % divisor, 0);
    assert_eq!(unpad_message(padded).unwrap(), message);
}

#[rstest]
#[case(vec![])] // empty
#[case(vec![0x80])] // incomplete varint
#[case(make_message(100, &[0; 5]))] // claims 100, has 5
#[case(make_message(10, &[0; 9]))] // claims 10, has 9
#[case(make_message(usize::MAX, &[0; 10]))] // integer overflow attempt
#[case(vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01])] // max varint
#[case(make_message(usize::MAX - 5, &[0; 10]))] // near-overflow attack
fn test_unpad_errors(#[case] invalid: Vec<u8>) {
    assert!(matches!(unpad_message(invalid), Err(ReconstructionError::MessagePaddingError(_))));
}

#[rstest]
#[case(make_message(0, &[]), vec![])] // zero length
#[case(make_message(3, &[1, 2, 3]), vec![1, 2, 3])] // exact
#[case(make_message(3, &[1, 2, 3, 0, 0]), vec![1, 2, 3])] // with padding
fn test_unpad_success(#[case] input: Vec<u8>, #[case] expected: Vec<u8>) {
    assert_eq!(unpad_message(input).unwrap(), expected);
}
