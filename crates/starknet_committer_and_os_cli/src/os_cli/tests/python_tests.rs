use std::collections::HashMap;

use blockifier::state::cached_state::StateMaps;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_os::test_utils::errors::OsSpecificTestError;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Blake2Felt252;

use crate::os_cli::commands::{validate_os_input, AggregatorCliInput, OsCliInput};
use crate::os_cli::tests::types::{OsPythonTestError, OsPythonTestResult};
use crate::shared_utils::types::{PythonTestError, PythonTestRunner};

// Enum representing different Python tests.
pub enum OsPythonTestRunner {
    AggregatorInputDeserialization,
    OsInputDeserialization,
    EncodeFelts,
    StateMapsSerialize,
}

// Implements conversion from a string to the test runner.
impl TryFrom<String> for OsPythonTestRunner {
    type Error = OsPythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "aggregator_input_deserialization" => Ok(Self::AggregatorInputDeserialization),
            "os_input_deserialization" => Ok(Self::OsInputDeserialization),
            "encode_felts" => Ok(Self::EncodeFelts),
            "state_maps_serialize_test" => Ok(Self::StateMapsSerialize),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

impl PythonTestRunner for OsPythonTestRunner {
    type SpecificError = OsSpecificTestError;
    async fn run(&self, input: Option<&str>) -> OsPythonTestResult {
        match self {
            Self::AggregatorInputDeserialization => {
                aggregator_input_deserialization(Self::non_optional_input(input)?)
            }
            Self::OsInputDeserialization => {
                os_input_deserialization(Self::non_optional_input(input)?)
            }
            Self::EncodeFelts => {
                let felts: Vec<Felt> = serde_json::from_str(Self::non_optional_input(input)?)?;
                Ok(format!("{:?}", Blake2Felt252::encode_felts_to_u32s(&felts)))
            }
            Self::StateMapsSerialize => state_maps_serialize_test(),
        }
    }
}

/// Serializes a deterministic blockifier [`StateMaps`] (the wire form of the block's
/// initial reads), mirrored by `test_state_maps_serde` in the python rust_vm_os_test.
fn state_maps_serialize_test() -> OsPythonTestResult {
    let first_contract = ContractAddress::from(0x1234_u128);
    let second_contract = ContractAddress::from(0x5678_u128);
    let first_class_hash = ClassHash(Felt::from(0xAA_u128));
    let second_class_hash = ClassHash(Felt::from(0xBB_u128));
    let state_maps = StateMaps {
        nonces: HashMap::from([
            (first_contract, Nonce(Felt::from(0x1_u128))),
            (second_contract, Nonce(Felt::from(0x0_u128))),
        ]),
        class_hashes: HashMap::from([
            (first_contract, first_class_hash),
            (second_contract, second_class_hash),
        ]),
        storage: HashMap::from([
            ((first_contract, StorageKey::from(0x1_u128)), Felt::from(0x64_u128)),
            ((first_contract, StorageKey::from(0x2_u128)), Felt::from(0xC8_u128)),
            ((second_contract, StorageKey::from(0x3_u128)), Felt::from(0x12C_u128)),
        ]),
        compiled_class_hashes: HashMap::from([(
            first_class_hash,
            CompiledClassHash(Felt::from(0xA00_u128)),
        )]),
        declared_contracts: HashMap::from([(first_class_hash, true), (second_class_hash, false)]),
    };
    Ok(serde_json::to_string(&state_maps)?)
}

/// Deserialize the OS input string into an `OsInput` struct.
fn os_input_deserialization(input_str: &str) -> OsPythonTestResult {
    let input = serde_json::from_str::<OsCliInput>(input_str)?;
    validate_os_input(&input.os_hints.os_input);
    Ok("Deserialization successful".to_string())
}

/// Deserialize the aggregator input string into an `AggregatorInput` struct.
fn aggregator_input_deserialization(input_str: &str) -> OsPythonTestResult {
    let _input = serde_json::from_str::<AggregatorCliInput>(input_str)?;
    // TODO(Aner): Validate the aggregator input.
    Ok("Deserialization successful".to_string())
}
