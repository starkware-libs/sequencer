use std::collections::HashMap;
use std::fmt::Debug;

use ethnum::U256;
use serde_json::json;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_committer::block_committer::input::{
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::forest::filled_forest::FilledForest;
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::tree::OriginalSkeletonStorageTrieConfig;
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::external_test_utils::single_tree_flow_test;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePathLength,
    NodeData,
    PathToBottom,
};
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_patricia_storage::db_object::DBObject;
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue, Storage};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Pedersen, StarkHash};
use thiserror;
use tracing::{debug, error, info, warn};

use super::utils::parse_from_python::TreeFlowInput;
use crate::committer_cli::filled_tree_output::filled_forest::SerializedForest;
use crate::committer_cli::parse_input::cast::CommitterInputImpl;
use crate::committer_cli::parse_input::read::parse_input;
use crate::committer_cli::tests::utils::parse_from_python::parse_input_single_storage_tree_flow_test;
use crate::committer_cli::tests::utils::random_structs::DummyRandomValue;
use crate::shared_utils::types::{PythonTestError, PythonTestResult, PythonTestRunner};

pub type CommitterPythonTestError = PythonTestError<CommitterSpecificTestError>;
pub type CommitterPythonTestResult = PythonTestResult<CommitterSpecificTestError>;

// Enum representing different Python tests.
pub enum CommitterPythonTestRunner {
    ExampleTest,
    FeltSerialize,
    HashFunction,
    BinarySerialize,
    InputParsing,
    NodeKey,
    StorageSerialize,
    ComparePythonHashConstants,
    StorageNode,
    FilledForestOutput,
    TreeHeightComparison,
    SerializeForRustCommitterFlowTest,
    ComputeHashSingleTree,
    MaybePanic,
    LogError,
}

/// Error type for CommitterPythonTest enum.
#[derive(Debug, thiserror::Error)]
pub enum CommitterSpecificTestError {
    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error("{0}")]
    KeyNotFound(String),
    #[error(transparent)]
    InvalidCastError(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    DeserializationTestFailure(#[from] DeserializationError),
}

/// Implements conversion from a string to the test runner.
impl TryFrom<String> for CommitterPythonTestRunner {
    type Error = CommitterPythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "example_test" => Ok(Self::ExampleTest),
            "felt_serialize_test" => Ok(Self::FeltSerialize),
            "hash_function_test" => Ok(Self::HashFunction),
            "binary_serialize_test" => Ok(Self::BinarySerialize),
            "input_parsing" => Ok(Self::InputParsing),
            "node_db_key_test" => Ok(Self::NodeKey),
            "storage_serialize_test" => Ok(Self::StorageSerialize),
            "compare_python_hash_constants" => Ok(Self::ComparePythonHashConstants),
            "storage_node_test" => Ok(Self::StorageNode),
            "filled_forest_output" => Ok(Self::FilledForestOutput),
            "compare_tree_height" => Ok(Self::TreeHeightComparison),
            "serialize_to_rust_committer_flow_test" => Ok(Self::SerializeForRustCommitterFlowTest),
            "tree_test" => Ok(Self::ComputeHashSingleTree),
            "maybe_panic" => Ok(Self::MaybePanic),
            "log_error" => Ok(Self::LogError),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

impl PythonTestRunner for CommitterPythonTestRunner {
    type SpecificError = CommitterSpecificTestError;

    /// Runs the test with the given arguments.
    async fn run(&self, input: Option<&str>) -> CommitterPythonTestResult {
        match self {
            Self::ExampleTest => {
                let example_input: HashMap<String, String> =
                    serde_json::from_str(Self::non_optional_input(input)?)?;
                Ok(example_test(example_input))
            }
            Self::FeltSerialize => {
                let felt = Self::non_optional_input(input)?.parse::<u128>().map_err(|err| {
                    PythonTestError::SpecificError(CommitterSpecificTestError::ParseIntError(err))
                })?;
                Ok(felt_serialize_test(felt))
            }
            Self::HashFunction => {
                let hash_input: HashMap<String, u128> =
                    serde_json::from_str(Self::non_optional_input(input)?)?;
                Ok(test_hash_function(hash_input))
            }
            Self::BinarySerialize => {
                let binary_input: HashMap<String, u128> =
                    serde_json::from_str(Self::non_optional_input(input)?)?;
                Ok(test_binary_serialize_test(binary_input))
            }
            Self::InputParsing => {
                let committer_input = serde_json::from_str(Self::non_optional_input(input)?)?;
                parse_input_test(committer_input)
            }
            Self::StorageSerialize => storage_serialize_test(),
            Self::NodeKey => Ok(test_node_db_key()),
            Self::ComparePythonHashConstants => Ok(python_hash_constants_compare()),
            Self::StorageNode => {
                let storage_node_input: HashMap<String, String> =
                    serde_json::from_str(Self::non_optional_input(input)?)?;
                test_storage_node(storage_node_input)
            }
            Self::FilledForestOutput => filled_forest_output_test(),
            Self::TreeHeightComparison => Ok(get_actual_tree_height()),
            Self::SerializeForRustCommitterFlowTest => {
                // TODO(Aner, 8/7/2024): refactor using structs for deserialization.
                let input: HashMap<String, String> =
                    serde_json::from_str(Self::non_optional_input(input)?)?;
                Ok(serialize_for_rust_committer_flow_test(input))
            }
            Self::ComputeHashSingleTree => {
                // 1. Get and deserialize input.
                let TreeFlowInput { leaf_modifications, storage, root_hash } =
                    serde_json::from_str(Self::non_optional_input(input)?)?;
                // 2. Run the test.
                let output = single_tree_flow_test::<StarknetStorageValue, TreeHashFunctionImpl>(
                    leaf_modifications,
                    &storage,
                    root_hash,
                    OriginalSkeletonStorageTrieConfig::new(false),
                )
                .await;
                // 3. Serialize and return output.
                Ok(output)
            }
            Self::MaybePanic => {
                let is_panic: bool = serde_json::from_str(Self::non_optional_input(input)?)?;
                if is_panic {
                    panic!("panic test")
                }
                Ok("Done!".to_owned())
            }
            Self::LogError => {
                error!("This is an error log message.");
                warn!("This is a warn log message.");
                info!("This is an info log message.");
                debug!("This is a debug log message.");
                panic!("This is a panic message.");
            }
        }
    }
}

// Test that the fetching of the input to flow test is working.
// TODO(Aner, 8/7/2024): refactor using structs for deserialization and rename the function.
fn serialize_for_rust_committer_flow_test(input: HashMap<String, String>) -> String {
    let TreeFlowInput { leaf_modifications, storage, root_hash } =
        parse_input_single_storage_tree_flow_test(&input);
    // Serialize the leaf modifications to an object that can be JSON-serialized.
    let leaf_modifications_to_print: HashMap<String, Vec<u8>> =
        leaf_modifications.into_iter().map(|(k, v)| (k.0.to_string(), v.serialize().0)).collect();

    // Create a json string to compare with the expected string in python.
    serde_json::to_string(&json!(
        {"leaf_modifications": leaf_modifications_to_print, "storage": storage, "root_hash": root_hash.0}
    )).expect("serialization failed")
}

fn get_or_key_not_found<'a, T: Debug>(
    map: &'a HashMap<String, T>,
    key: &'a str,
) -> Result<&'a T, CommitterPythonTestError> {
    map.get(key).ok_or_else(|| {
        PythonTestError::SpecificError(CommitterSpecificTestError::KeyNotFound(format!(
            "Failed to get value for key '{key}' from {map:?}."
        )))
    })
}

fn get_actual_tree_height() -> String {
    SubTreeHeight::ACTUAL_HEIGHT.to_string()
}

pub(crate) fn example_test(test_args: HashMap<String, String>) -> String {
    let x = test_args.get("x").expect("Failed to get value for key 'x'");
    let y = test_args.get("y").expect("Failed to get value for key 'y'");
    format!("Calling example test with args: x: {x}, y: {y}")
}

/// Serializes a Felt into a string.
pub(crate) fn felt_serialize_test(felt: u128) -> String {
    let bytes = Felt::from(felt).to_bytes_be().to_vec();
    serde_json::to_string(&bytes)
        .unwrap_or_else(|error| panic!("Failed to serialize felt: {error}"))
}

pub(crate) fn test_hash_function(hash_input: HashMap<String, u128>) -> String {
    // Fetch x and y from the input.
    let x = hash_input.get("x").expect("Failed to get value for key 'x'");
    let y = hash_input.get("y").expect("Failed to get value for key 'y'");

    // Convert x and y to Felt.
    let x_felt = Felt::from(*x);
    let y_felt = Felt::from(*y);

    // Compute the hash.
    let hash_result = Pedersen::hash(&x_felt, &y_felt);

    // Serialize the hash result.
    serde_json::to_string(&hash_result)
        .unwrap_or_else(|error| panic!("Failed to serialize hash result: {error}"))
}

/// Serializes binary data into a JSON string.
/// # Arguments
///
/// * `left` - The left 128-bit integer used to create binary data.
/// * `right` - The right 128-bit integer used to create binary data.
///
/// # Returns
///
/// A JSON string representing the value of serialized binary data.
pub(crate) fn test_binary_serialize_test(binary_input: HashMap<String, u128>) -> String {
    // Extract left and right values from the input.
    let left = binary_input.get("left").expect("Failed to get value for key 'left'");
    let right = binary_input.get("right").expect("Failed to get value for key 'right'");

    // Create a map to store the serialized binary data.
    let mut map: HashMap<String, Vec<u8>> = HashMap::new();

    // Create binary data from the left and right values.
    let binary_data = BinaryData {
        left_hash: HashOutput(Felt::from(*left)),
        right_hash: HashOutput(Felt::from(*right)),
    };

    // Create a filled node (irrelevant leaf type) with binary data and zero hash.
    let filled_node: FilledNode<StarknetStorageValue> =
        FilledNode { data: NodeData::Binary(binary_data), hash: HashOutput(Felt::ZERO) };

    // Serialize the binary node and insert it into the map under the key "value".
    let value = filled_node.serialize();
    map.insert("value".to_string(), value.0);

    // Serialize the map to a JSON string and handle serialization errors.
    serde_json::to_string(&map)
        .unwrap_or_else(|error| panic!("Failed to serialize binary fact: {error}"))
}

pub(crate) fn parse_input_test(committer_input: String) -> CommitterPythonTestResult {
    Ok(create_output_to_python(parse_input(&committer_input).map_err(|err| {
        PythonTestError::SpecificError(CommitterSpecificTestError::DeserializationTestFailure(err))
    })?))
}

fn create_output_to_python(
    CommitterInputImpl { input: actual_input, storage }: CommitterInputImpl,
) -> String {
    let (storage_keys_hash, storage_values_hash) = hash_storage(&storage);
    let (state_diff_keys_hash, state_diff_values_hash) = hash_state_diff(&actual_input.state_diff);
    format!(
        r#"
        {{
        "storage_size": {},
        "address_to_class_hash_size": {},
        "address_to_nonce_size": {},
        "class_hash_to_compiled_class_hash": {},
        "outer_storage_updates_size": {},
        "global_tree_root_hash": {:?},
        "classes_tree_root_hash": {:?},
        "storage_keys_hash": {:?},
        "storage_values_hash": {:?},
        "state_diff_keys_hash": {:?},
        "state_diff_values_hash": {:?}
        }}"#,
        storage.len(),
        actual_input.state_diff.address_to_class_hash.len(),
        actual_input.state_diff.address_to_nonce.len(),
        actual_input.state_diff.class_hash_to_compiled_class_hash.len(),
        actual_input.state_diff.storage_updates.len(),
        actual_input.contracts_trie_root_hash.0.to_bytes_be(),
        actual_input.classes_trie_root_hash.0.to_bytes_be(),
        storage_keys_hash,
        storage_values_hash,
        state_diff_keys_hash,
        state_diff_values_hash
    )
}

/// Calculates the 'hash' of the parsed state diff in order to verify the state diff sent
/// from python was parsed correctly.
fn hash_state_diff(state_diff: &StateDiff) -> (Vec<u8>, Vec<u8>) {
    let (address_to_class_hash_keys_hash, address_to_class_hash_values_hash) =
        hash_address_to_class_hash(&state_diff.address_to_class_hash);
    let (address_to_nonce_keys_hash, address_to_nonce_values_hash) =
        hash_address_to_nonce(&state_diff.address_to_nonce);
    let (
        class_hash_to_compiled_class_hash_keys_hash,
        class_hash_to_compiled_class_hash_values_hash,
    ) = hash_class_hash_to_compiled_class_hash(&state_diff.class_hash_to_compiled_class_hash);
    let (storage_updates_keys_hash, storage_updates_values_hash) =
        hash_storage_updates(&state_diff.storage_updates);
    let mut state_diff_keys_hash =
        xor_hash(&address_to_class_hash_keys_hash, &address_to_nonce_keys_hash);
    state_diff_keys_hash =
        xor_hash(&state_diff_keys_hash, &class_hash_to_compiled_class_hash_keys_hash);
    state_diff_keys_hash = xor_hash(&state_diff_keys_hash, &storage_updates_keys_hash);
    let mut state_diff_values_hash =
        xor_hash(&address_to_class_hash_values_hash, &address_to_nonce_values_hash);
    state_diff_values_hash =
        xor_hash(&state_diff_values_hash, &class_hash_to_compiled_class_hash_values_hash);
    state_diff_values_hash = xor_hash(&state_diff_values_hash, &storage_updates_values_hash);
    (state_diff_keys_hash, state_diff_values_hash)
}

fn hash_storage_updates(
    storage_updates: &HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>>,
) -> (Vec<u8>, Vec<u8>) {
    let mut keys_hash = vec![0; 32];
    let mut values_hash = vec![0; 32];
    for (key, inner_map) in storage_updates {
        keys_hash = xor_hash(&keys_hash, &key.0.to_bytes_be());
        let (inner_map_keys_hash, inner_map_values_hash) = hash_storage_updates_map(inner_map);
        values_hash = xor_hash(&values_hash, &inner_map_keys_hash);
        values_hash = xor_hash(&values_hash, &inner_map_values_hash);
    }
    (keys_hash, values_hash)
}

macro_rules! generate_storage_map_xor_hasher {
    ($fn_name:ident, $key_type:ty, $val_type:ty) => {
        fn $fn_name(inner_map: &HashMap<$key_type, $val_type>) -> (Vec<u8>, Vec<u8>) {
            let mut keys_hash = vec![0; 32];
            let mut values_hash = vec![0; 32];
            for (key, value) in inner_map {
                keys_hash = xor_hash(&keys_hash, &key.0.to_bytes_be());
                values_hash = xor_hash(&values_hash, &value.0.to_bytes_be());
            }
            (keys_hash, values_hash)
        }
    };
}

generate_storage_map_xor_hasher!(
    hash_storage_updates_map,
    StarknetStorageKey,
    StarknetStorageValue
);
generate_storage_map_xor_hasher!(
    hash_class_hash_to_compiled_class_hash,
    ClassHash,
    CompiledClassHash
);
generate_storage_map_xor_hasher!(hash_address_to_class_hash, ContractAddress, ClassHash);
generate_storage_map_xor_hasher!(hash_address_to_nonce, ContractAddress, Nonce);

fn hash_storage(storage: &MapStorage) -> (Vec<u8>, Vec<u8>) {
    let mut keys_hash = vec![0; 32];
    let mut values_hash = vec![0; 32];
    for (key, value) in storage {
        keys_hash = xor_hash(&keys_hash, &key.0);
        values_hash = xor_hash(&values_hash, &value.0);
    }
    (keys_hash, values_hash)
}

fn xor_hash(x: &[u8], y: &[u8]) -> Vec<u8> {
    x.iter().zip(y.iter()).map(|(a, b)| a ^ b).collect()
}

/// Creates and serializes storage keys for different node types.
///
/// This function generates and serializes storage keys for various node types, including binary
/// nodes, edge nodes, storage leaf nodes, state tree leaf nodes, and compiled class leaf nodes. The
/// resulting keys are stored in a `HashMap` and serialized into a JSON string.
///
/// # Returns
///
/// A JSON string representing the serialized storage keys for different node types.
pub(crate) fn test_node_db_key() -> String {
    let zero = Felt::ZERO;

    // Generate keys for different node types.
    let hash = HashOutput(zero);

    let binary_node: FilledNode<StarknetStorageValue> = FilledNode {
        data: NodeData::Binary(BinaryData { left_hash: hash, right_hash: hash }),
        hash,
    };
    let binary_node_key = binary_node.db_key().0;

    let edge_node: FilledNode<StarknetStorageValue> = FilledNode {
        data: NodeData::Edge(EdgeData { bottom_hash: hash, path_to_bottom: Default::default() }),
        hash,
    };

    let edge_node_key = edge_node.db_key().0;

    let storage_leaf = FilledNode { data: NodeData::Leaf(StarknetStorageValue(zero)), hash };
    let storage_leaf_key = storage_leaf.db_key().0;

    let state_tree_leaf = FilledNode {
        data: NodeData::Leaf(ContractState {
            class_hash: ClassHash(zero),
            storage_root_hash: HashOutput(zero),
            nonce: Nonce(zero),
        }),
        hash,
    };
    let state_tree_leaf_key = state_tree_leaf.db_key().0;

    let compiled_class_leaf = FilledNode { data: NodeData::Leaf(CompiledClassHash(zero)), hash };
    let compiled_class_leaf_key = compiled_class_leaf.db_key().0;

    // Store keys in a HashMap.
    let mut map: HashMap<String, Vec<u8>> = HashMap::new();

    map.insert("binary_node_key".to_string(), binary_node_key);
    map.insert("edge_node_key".to_string(), edge_node_key);
    map.insert("storage_leaf_key".to_string(), storage_leaf_key);
    map.insert("state_tree_leaf_key".to_string(), state_tree_leaf_key);
    map.insert("compiled_class_leaf_key".to_string(), compiled_class_leaf_key);

    // Serialize the map to a JSON string and handle serialization errors.
    serde_json::to_string(&map)
        .unwrap_or_else(|error| panic!("Failed to serialize storage prefix: {error}"))
}

/// This function storage_serialize_test generates a MapStorage containing DbKey and
/// DbValue pairs for u128 values in the range 0..=1000,
/// serializes it to a JSON string using Serde,
/// and returns the serialized JSON string or panics with an error message if serialization fails.
pub(crate) fn storage_serialize_test() -> CommitterPythonTestResult {
    let mut storage = HashMap::new();
    for i in 0..=99_u128 {
        let key = DbKey(Felt::from(i).to_bytes_be().to_vec());
        let value = DbValue(Felt::from(i).to_bytes_be().to_vec());
        storage.set(key, value);
    }

    Ok(serde_json::to_string(&storage)?)
}

fn python_hash_constants_compare() -> String {
    format!(
        "[{:?}, {:?}]",
        TreeHashFunctionImpl::CONTRACT_STATE_HASH_VERSION.to_bytes_be(),
        Felt::from_hex(TreeHashFunctionImpl::CONTRACT_CLASS_LEAF_V0).expect(
        "could not parse hex string corresponding to b'CONTRACT_CLASS_LEAF_V0' to Felt",
        ).to_bytes_be()
    )
}

/// Processes a map containing JSON strings for different node data.
/// Creates `NodeData` objects for each node type, stores them in a storage, and serializes the map
/// to a JSON string.
///
/// # Arguments
/// * `data` - A map containing JSON strings for different node data:
///   - `"binary"`: Binary node data.
///   - `"edge"`: Edge node data.
///   - `"storage"`: Storage leaf data.
///   - `"contract_state_leaf"`: Contract state leaf data.
///   - `"contract_class_leaf"`: Compiled class leaf data.
///
/// # Returns
/// A `Result<String, CommitterTestError>` containing a serialized map of all nodes on
/// success, or an error if keys are missing or parsing fails.
fn test_storage_node(data: HashMap<String, String>) -> CommitterPythonTestResult {
    // Create a storage to store the nodes.
    let mut rust_fact_storage = HashMap::new();

    // Parse the binary node data from the input.
    let binary_json = get_or_key_not_found(&data, "binary")?;
    let binary_data: HashMap<String, u128> = serde_json::from_str(binary_json)?;

    // Create a binary node from the parsed data.
    let binary_rust: FilledNode<StarknetStorageValue> = FilledNode {
        data: NodeData::Binary(BinaryData {
            left_hash: HashOutput(Felt::from(*get_or_key_not_found(&binary_data, "left")?)),
            right_hash: HashOutput(Felt::from(*get_or_key_not_found(&binary_data, "right")?)),
        }),
        hash: HashOutput(Felt::from(*get_or_key_not_found(&binary_data, "hash")?)),
    };

    // Store the binary node in the storage.
    rust_fact_storage.set(binary_rust.db_key(), binary_rust.serialize());

    // Parse the edge node data from the input.
    let edge_json = get_or_key_not_found(&data, "edge")?;
    let edge_data: HashMap<String, u128> = serde_json::from_str(edge_json)?;

    // Create an edge node from the parsed data.
    let edge_rust: FilledNode<StarknetStorageValue> = FilledNode {
        data: NodeData::Edge(EdgeData {
            bottom_hash: HashOutput(Felt::from(*get_or_key_not_found(&edge_data, "bottom")?)),
            path_to_bottom: PathToBottom::new(
                U256::from(*get_or_key_not_found(&edge_data, "path")?).into(),
                EdgePathLength::new(
                    (*get_or_key_not_found(&edge_data, "length")?).try_into().map_err(|err| {
                        PythonTestError::SpecificError(
                            CommitterSpecificTestError::InvalidCastError(err),
                        )
                    })?,
                )
                .expect("Invalid length"),
            )
            .expect("Illegal PathToBottom"),
        }),
        hash: HashOutput(Felt::from(*get_or_key_not_found(&edge_data, "hash")?)),
    };

    // Store the edge node in the storage.
    rust_fact_storage.set(edge_rust.db_key(), edge_rust.serialize());

    // Parse the storage leaf data from the input.
    let storage_leaf_json = get_or_key_not_found(&data, "storage")?;
    let storage_leaf_data: HashMap<String, u128> = serde_json::from_str(storage_leaf_json)?;

    // Create a storage leaf node from the parsed data.
    let storage_leaf_rust = FilledNode {
        data: NodeData::Leaf(StarknetStorageValue(Felt::from(*get_or_key_not_found(
            &storage_leaf_data,
            "value",
        )?))),
        hash: HashOutput(Felt::from(*get_or_key_not_found(&storage_leaf_data, "hash")?)),
    };

    // Store the storage leaf node in the storage.
    rust_fact_storage.set(storage_leaf_rust.db_key(), storage_leaf_rust.serialize());

    // Parse the contract state leaf data from the input.
    let contract_state_leaf = get_or_key_not_found(&data, "contract_state_leaf")?;
    let contract_state_leaf_data: HashMap<String, u128> =
        serde_json::from_str(contract_state_leaf)?;

    // Create a contract state leaf node from the parsed data.
    let contract_state_leaf_rust = FilledNode {
        data: NodeData::Leaf(ContractState {
            class_hash: ClassHash(Felt::from(*get_or_key_not_found(
                &contract_state_leaf_data,
                "contract_hash",
            )?)),
            storage_root_hash: HashOutput(Felt::from(*get_or_key_not_found(
                &contract_state_leaf_data,
                "root",
            )?)),
            nonce: Nonce(Felt::from(*get_or_key_not_found(&contract_state_leaf_data, "nonce")?)),
        }),

        hash: HashOutput(Felt::from(*get_or_key_not_found(&contract_state_leaf_data, "hash")?)),
    };

    // Store the contract state leaf node in the storage.
    rust_fact_storage.set(contract_state_leaf_rust.db_key(), contract_state_leaf_rust.serialize());

    // Parse the compiled class leaf data from the input.
    let compiled_class_leaf = get_or_key_not_found(&data, "contract_class_leaf")?;
    let compiled_class_leaf_data: HashMap<String, u128> =
        serde_json::from_str(compiled_class_leaf)?;

    // Create a compiled class leaf node from the parsed data.
    let compiled_class_leaf_rust = FilledNode {
        data: NodeData::Leaf(CompiledClassHash(Felt::from(*get_or_key_not_found(
            &compiled_class_leaf_data,
            "compiled_class_hash",
        )?))),
        hash: HashOutput(Felt::from(*get_or_key_not_found(&compiled_class_leaf_data, "hash")?)),
    };

    // Store the compiled class leaf node in the storage.
    rust_fact_storage.set(compiled_class_leaf_rust.db_key(), compiled_class_leaf_rust.serialize());

    // Serialize the storage to a JSON string and handle serialization errors.
    Ok(serde_json::to_string(&rust_fact_storage)?)
}

/// Generates a dummy random filled forest and serializes it to a JSON string.
pub(crate) fn filled_forest_output_test() -> CommitterPythonTestResult {
    let dummy_forest = SerializedForest(FilledForest::dummy_random(&mut rand::thread_rng(), None));
    let output = dummy_forest.forest_to_output();
    let output_string = serde_json::to_string(&output).expect("Failed to serialize");
    Ok(output_string)
}
