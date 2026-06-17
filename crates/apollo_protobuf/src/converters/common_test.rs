use starknet_types_core::felt::Felt;

use crate::converters::ProtobufConversionError;
use crate::protobuf;

#[test]
fn felt252_round_trip() {
    let felt = Felt::from(0x1234_5678_u64);
    let protobuf_felt = protobuf::Felt252::from(felt);
    let result = Felt::try_from(protobuf_felt).unwrap();
    assert_eq!(felt, result);
}

#[test]
fn felt252_too_few_bytes_returns_error() {
    assert_length_mismatch(Felt::try_from(protobuf::Felt252 { elements: vec![0; 31] }));
}

#[test]
fn felt252_too_many_bytes_returns_error() {
    assert_length_mismatch(Felt::try_from(protobuf::Felt252 { elements: vec![0; 33] }));
}

#[test]
fn felt252_empty_bytes_returns_error() {
    assert_length_mismatch(Felt::try_from(protobuf::Felt252 { elements: vec![] }));
}

fn assert_length_mismatch(result: Result<Felt, ProtobufConversionError>) {
    assert!(
        matches!(
            result,
            Err(ProtobufConversionError::BytesDataLengthMismatch {
                type_description: "Felt252",
                num_expected: 32,
                ..
            })
        ),
        "expected BytesDataLengthMismatch for Felt252, got {result:?}",
    );
}
