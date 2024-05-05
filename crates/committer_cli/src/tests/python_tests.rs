use committer::block_committer::types::{
    ContractAddress, ContractState, Input, StarknetStorageKey, StarknetStorageValue, StateDiff,
};
use committer::felt::Felt;
use committer::hash::hash_trait::HashOutput;
use committer::hash::hash_trait::{HashFunction, HashInputPair};
use committer::hash::pedersen::PedersenHashFunction;
use committer::patricia_merkle_tree::filled_tree::node::CompiledClassHash;
use committer::patricia_merkle_tree::filled_tree::node::{ClassHash, FilledNode, Nonce};
use committer::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData, NodeData};
use committer::patricia_merkle_tree::node_data::leaf::LeafDataImpl;
use committer::storage::errors::DeserializationError;
use committer::storage::map_storage::MapStorage;
use committer::storage::serde_trait::Serializable;
use committer::storage::storage_trait::{Storage, StorageKey, StorageValue};
use std::{collections::HashMap, io};
use thiserror;

use crate::parse_input::read::parse_input;

// Enum representing different Python tests.
pub(crate) enum PythonTest {
    ExampleTest,
    FeltSerialize,
    HashFunction,
    BinarySerialize,
    InputParsing,
    NodeKey,
    StorageSerialize,
}

/// Error type for PythonTest enum.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PythonTestError {
    #[error("Unknown test name: {0}")]
    UnknownTestName(String),
    #[error("Failed to parse input: {0}")]
    ParseInputError(#[from] serde_json::Error),
    #[error("Failed to parse integer input: {0}")]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error("Test failed. {0}")]
    DeserializationTestFailure(#[from] DeserializationError),
    #[error("Failed to read from stdin.")]
    StdinReadError(#[from] io::Error),
}

/// Implements conversion from a string to a `PythonTest`.
impl TryFrom<String> for PythonTest {
    type Error = PythonTestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "example_test" => Ok(Self::ExampleTest),
            "felt_serialize_test" => Ok(Self::FeltSerialize),
            "hash_function_test" => Ok(Self::HashFunction),
            "binary_serialize_test" => Ok(Self::BinarySerialize),
            "input_parsing" => Ok(Self::InputParsing),
            "node_db_key_test" => Ok(Self::NodeKey),
            "storage_serialize_test" => Ok(Self::StorageSerialize),
            _ => Err(PythonTestError::UnknownTestName(value)),
        }
    }
}

impl PythonTest {
    /// Runs the test with the given arguments.
    pub(crate) fn run(&self, input: &str) -> Result<String, PythonTestError> {
        match self {
            Self::ExampleTest => {
                let example_input: HashMap<String, String> = serde_json::from_str(input)?;
                Ok(example_test(example_input))
            }
            Self::FeltSerialize => {
                let felt = input.parse::<u128>()?;
                Ok(felt_serialize_test(felt))
            }
            Self::HashFunction => {
                let hash_input: HashMap<String, u128> = serde_json::from_str(input)?;
                Ok(test_hash_function(hash_input))
            }
            Self::BinarySerialize => {
                let binary_input: HashMap<String, u128> = serde_json::from_str(input)?;
                Ok(test_binary_serialize_test(binary_input))
            }
            Self::InputParsing => parse_input_test(),
            Self::StorageSerialize => storage_serialize_test(),
            Self::NodeKey => Ok(test_node_db_key()),
        }
    }
}

pub(crate) fn example_test(test_args: HashMap<String, String>) -> String {
    let x = test_args.get("x").expect("Failed to get value for key 'x'");
    let y = test_args.get("y").expect("Failed to get value for key 'y'");
    format!("Calling example test with args: x: {}, y: {}", x, y)
}

/// Serializes a Felt into a string.
pub(crate) fn felt_serialize_test(felt: u128) -> String {
    let bytes = Felt::from(felt).as_bytes().to_vec();
    serde_json::to_string(&bytes)
        .unwrap_or_else(|error| panic!("Failed to serialize felt: {}", error))
}

pub(crate) fn test_hash_function(hash_input: HashMap<String, u128>) -> String {
    // Fetch x and y from the input.
    let x = hash_input
        .get("x")
        .expect("Failed to get value for key 'x'");
    let y = hash_input
        .get("y")
        .expect("Failed to get value for key 'y'");

    // Convert x and y to Felt.
    let x_felt = Felt::from(*x);
    let y_felt = Felt::from(*y);

    // Compute the hash.
    let hash_result = PedersenHashFunction::compute_hash(HashInputPair(x_felt, y_felt)).0;

    // Serialize the hash result.
    serde_json::to_string(&hash_result)
        .unwrap_or_else(|error| panic!("Failed to serialize hash result: {}", error))
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
    let left = binary_input
        .get("left")
        .expect("Failed to get value for key 'left'");
    let right = binary_input
        .get("right")
        .expect("Failed to get value for key 'right'");

    // Create a map to store the serialized binary data.
    let mut map: HashMap<String, Vec<u8>> = HashMap::new();

    // Create binary data from the left and right values.
    let binary_data = BinaryData {
        left_hash: HashOutput(Felt::from(*left)),
        right_hash: HashOutput(Felt::from(*right)),
    };

    // Create a filled node with binary data and zero hash.
    let filled_node = FilledNode {
        data: NodeData::Binary(binary_data),
        hash: HashOutput(Felt::ZERO),
    };

    // Serialize the binary node and insert it into the map under the key "value".
    let value = filled_node
        .serialize()
        .unwrap_or_else(|error| panic!("Failed to serialize binary data: {}", error));
    map.insert("value".to_string(), value.0);

    // Serialize the map to a JSON string and handle serialization errors.
    serde_json::to_string(&map)
        .unwrap_or_else(|error| panic!("Failed to serialize binary fact: {}", error))
}

pub(crate) fn parse_input_test() -> Result<String, PythonTestError> {
    let input = io::read_to_string(io::stdin())?;
    Ok(create_output_to_python(parse_input(input)?))
}

fn create_output_to_python(actual_input: Input) -> String {
    let (storage_keys_hash, storage_values_hash) = hash_storage(&actual_input.storage);
    let (state_diff_keys_hash, state_diff_values_hash) = hash_state_diff(&actual_input.state_diff);
    format!(
        r#"
        {{
        "storage_size": {},
        "address_to_class_hash_size": {},
        "address_to_nonce_size": {},
        "class_hash_to_compiled_class_hash": {},
        "current_contract_state_leaves_size": {},
        "outer_storage_updates_size": {},
        "tree_height": {},
        "global_tree_root_hash": {:?},
        "classes_tree_root_hash": {:?},
        "storage_keys_hash": {:?},
        "storage_values_hash": {:?},
        "state_diff_keys_hash": {:?},
        "state_diff_values_hash": {:?}
        }}"#,
        actual_input.storage.len(),
        actual_input.state_diff.address_to_class_hash.len(),
        actual_input.state_diff.address_to_nonce.len(),
        actual_input
            .state_diff
            .class_hash_to_compiled_class_hash
            .len(),
        actual_input.state_diff.current_contract_state_leaves.len(),
        actual_input.state_diff.storage_updates.len(),
        actual_input.tree_height.0,
        actual_input.global_tree_root_hash.0.to_bytes_be(),
        actual_input.classes_tree_root_hash.0.to_bytes_be(),
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
    let (current_contract_states_keys_hash, current_contract_states_values_hash) =
        hash_current_contract_states(&state_diff.current_contract_state_leaves);
    let mut state_diff_keys_hash = xor_hash(
        &address_to_class_hash_keys_hash,
        &address_to_nonce_keys_hash,
    );
    state_diff_keys_hash = xor_hash(
        &state_diff_keys_hash,
        &class_hash_to_compiled_class_hash_keys_hash,
    );
    state_diff_keys_hash = xor_hash(&state_diff_keys_hash, &storage_updates_keys_hash);
    state_diff_keys_hash = xor_hash(&state_diff_keys_hash, &current_contract_states_keys_hash);
    let mut state_diff_values_hash = xor_hash(
        &address_to_class_hash_values_hash,
        &address_to_nonce_values_hash,
    );
    state_diff_values_hash = xor_hash(
        &state_diff_values_hash,
        &class_hash_to_compiled_class_hash_values_hash,
    );
    state_diff_values_hash = xor_hash(&state_diff_values_hash, &storage_updates_values_hash);
    state_diff_values_hash = xor_hash(
        &state_diff_values_hash,
        &current_contract_states_values_hash,
    );
    (state_diff_keys_hash, state_diff_values_hash)
}

fn hash_current_contract_states(
    current_contract_state_leaves: &HashMap<ContractAddress, ContractState>,
) -> (Vec<u8>, Vec<u8>) {
    let mut keys_hash = vec![0; 32];
    let mut values_hash = vec![0; 32];
    for (key, state_leaf) in current_contract_state_leaves {
        keys_hash = xor_hash(&keys_hash, &key.0.to_bytes_be());
        values_hash = xor_hash(&values_hash, &state_leaf.nonce.0.to_bytes_be());
        values_hash = xor_hash(&values_hash, &state_leaf.class_hash.0.to_bytes_be());
        values_hash = xor_hash(&values_hash, &state_leaf.storage_root_hash.0.to_bytes_be());
    }
    (keys_hash, values_hash)
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

fn hash_storage(storage: &HashMap<StorageKey, StorageValue>) -> (Vec<u8>, Vec<u8>) {
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
/// This function generates and serializes storage keys for various node types, including binary nodes,
/// edge nodes, storage leaf nodes, state tree leaf nodes, and compiled class leaf nodes. The resulting
/// keys are stored in a `HashMap` and serialized into a JSON string.
///
/// # Returns
///
/// A JSON string representing the serialized storage keys for different node types.
///
pub(crate) fn test_node_db_key() -> String {
    let zero = Felt::ZERO;

    // Generate keys for different node types.
    let hash = HashOutput(zero);

    let binary_node_key = FilledNode {
        data: NodeData::Binary(BinaryData {
            left_hash: hash,
            right_hash: hash,
        }),
        hash,
    }
    .db_key()
    .0;

    let edge_node_key = FilledNode {
        data: NodeData::Edge(EdgeData {
            bottom_hash: hash,
            path_to_bottom: Default::default(),
        }),
        hash,
    }
    .db_key()
    .0;

    let storage_leaf_key = FilledNode {
        data: NodeData::Leaf(LeafDataImpl::StorageValue(zero)),
        hash,
    }
    .db_key()
    .0;

    let state_tree_leaf_key = FilledNode {
        data: NodeData::Leaf(LeafDataImpl::StateTreeTuple {
            class_hash: ClassHash(zero),
            contract_state_root_hash: zero,
            nonce: Nonce(zero),
        }),
        hash,
    }
    .db_key()
    .0;

    let compiled_class_leaf_key = FilledNode {
        data: NodeData::Leaf(LeafDataImpl::CompiledClassHash(ClassHash(zero))),
        hash,
    }
    .db_key()
    .0;

    // Store keys in a HashMap.
    let mut map: HashMap<String, Vec<u8>> = HashMap::new();

    map.insert("binary_node_key".to_string(), binary_node_key);
    map.insert("edge_node_key".to_string(), edge_node_key);
    map.insert("storage_leaf_key".to_string(), storage_leaf_key);
    map.insert("state_tree_leaf_key".to_string(), state_tree_leaf_key);
    map.insert(
        "compiled_class_leaf_key".to_string(),
        compiled_class_leaf_key,
    );

    // Serialize the map to a JSON string and handle serialization errors.
    serde_json::to_string(&map)
        .unwrap_or_else(|error| panic!("Failed to serialize storage prefix: {}", error))
}

/// This function storage_serialize_test generates a MapStorage containing StorageKey and StorageValue
/// pairs for u128 values in the range 0..=1000,
/// serializes it to a JSON string using Serde,
/// and returns the serialized JSON string or panics with an error message if serialization fails.
pub(crate) fn storage_serialize_test() -> Result<String, PythonTestError> {
    let mut storage = MapStorage {
        storage: HashMap::new(),
    };
    for i in 0..=99_u128 {
        let key = StorageKey(Felt::from(i).as_bytes().to_vec());
        let value = StorageValue(Felt::from(i).as_bytes().to_vec());
        storage.set(key, value);
    }

    serde_json::to_string(&storage).map_err(PythonTestError::from)
}
