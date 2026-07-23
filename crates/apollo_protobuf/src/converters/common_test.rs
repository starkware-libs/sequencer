use rstest::rstest;
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

#[rstest]
#[case::too_few_bytes(vec![0; 31])]
#[case::too_many_bytes(vec![0; 33])]
#[case::empty(vec![])]
fn felt252_too_few_bytes_returns_error(#[case] elements: Vec<u8>) {
    let result = Felt::try_from(protobuf::Felt252 { elements });
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
