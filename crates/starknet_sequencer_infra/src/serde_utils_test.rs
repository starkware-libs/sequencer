use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;

use crate::serde_utils::SerdeWrapper;

fn test_generic_data_serde<T>(data: T)
where
    T: Serialize + for<'de> Deserialize<'de> + Debug + Clone + PartialEq,
{
    // Serialize and deserialize the data.
    let encoded = SerdeWrapper::new(data.clone()).wrapper_serialize().unwrap();
    let decoded = SerdeWrapper::<T>::wrapper_deserialize(&encoded).unwrap();

    // Assert that the data is the same after serialization and deserialization.
    assert_eq!(data, decoded);
}

#[test]
fn test_serde_native_type() {
    let data: u32 = 8;
    test_generic_data_serde(data);
}

#[test]
fn test_serde_struct_type() {
    #[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
    struct TestStruct {
        a: u32,
        b: u32,
    }

    let data: TestStruct = TestStruct { a: 17, b: 8 };
    test_generic_data_serde(data);
}

#[test]
fn test_serde_felt() {
    let data: Felt = Felt::ONE;
    test_generic_data_serde(data);
}
