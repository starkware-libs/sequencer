use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;

use crate::component_definitions::{BincodeSerializable, SerdeWrapper};
use crate::trace_util::configure_tracing;

#[test]
fn test_serde_native_type() {
    let data: u32 = 8;

    let encoded =
        SerdeWrapper { data }.to_bincode().expect("Server error serialization should succeed");
    let decoded = SerdeWrapper::<u32>::from_bincode(&encoded).unwrap();

    assert_eq!(data, decoded.data);
}

#[test]
fn test_serde_struct_type() {
    #[derive(Serialize, Deserialize, std::fmt::Debug, Clone, std::cmp::PartialEq, Copy)]
    struct TestStruct {
        a: u32,
        b: u32,
    }

    let data: TestStruct = TestStruct { a: 17, b: 8 };

    let encoded =
        SerdeWrapper { data }.to_bincode().expect("Server error serialization should succeed");
    let decoded = SerdeWrapper::<TestStruct>::from_bincode(&encoded).unwrap();

    assert_eq!(data, decoded.data);
}

#[test]
fn test_serde_felt() {
    configure_tracing();

    let data: Felt = Felt::ONE;

    let encoded =
        SerdeWrapper { data }.to_bincode().expect("Server error serialization should succeed");
    let decoded = SerdeWrapper::<Felt>::from_bincode(&encoded).unwrap();

    assert_eq!(data, decoded.data);
}
