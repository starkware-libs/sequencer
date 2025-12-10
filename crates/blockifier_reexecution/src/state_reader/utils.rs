use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs::read_to_string;
use std::sync::{Arc, LazyLock};

use apollo_gateway_config::config::RpcStateReaderConfig;
use apollo_rpc_execution::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::context::{ChainInfo, FeeTokenAddresses};
use blockifier::execution::contract_class::{CompiledClassV0, CompiledClassV1};
use blockifier::state::cached_state::{CommitmentStateDiff, StateMaps};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::StateResult;
use indexmap::IndexMap;
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

use crate::state_reader::cli::TransactionInput;
use crate::state_reader::errors::{ReexecutionError, ReexecutionResult};
use crate::state_reader::offline_state_reader::{
    OfflineConsecutiveStateReaders,
    SerializableDataPrevBlock,
    SerializableOfflineReexecutionData,
};
use crate::state_reader::reexecution_state_reader::{
    ConsecutiveReexecutionStateReaders,
    ReexecutionStateReader,
};
use crate::state_reader::serde_utils::deserialize_transaction_json_to_starknet_api_tx;
use crate::state_reader::test_state_reader::ConsecutiveTestStateReaders;

pub static RPC_NODE_URL: LazyLock<String> = LazyLock::new(|| {
    env::var("TEST_URL")
        .unwrap_or_else(|_| "https://free-rpc.nethermind.io/mainnet-juno/".to_string())
});

/// Converts a [`starknet_api::contract_class::ContractClass`] into the corresponding
/// [`CompiledClasses`].
///
/// For `V1` (Cairo 1) classes, a matching `SierraContractClass` must be provided.
/// For `V0` classes, this argument should be `None`.
pub fn contract_class_to_compiled_classes(
    contract_class: ContractClass,
    sierra_contract_class: Option<SierraContractClass>,
) -> StateResult<CompiledClasses> {
    match contract_class {
        ContractClass::V0(deprecated_class) => {
            Ok(CompiledClasses::V0(CompiledClassV0::try_from(deprecated_class)?))
        }
        ContractClass::V1(versioned_casm) => {
            let sierra_contract_class =
                sierra_contract_class.expect("V1 contract class requires Sierra class");
            Ok(CompiledClasses::V1(
                CompiledClassV1::try_from(versioned_casm)?,
                Arc::new(sierra_contract_class),
            ))
        }
    }
}

/// Returns the fee token addresses of mainnet.
pub fn get_fee_token_addresses(chain_id: &ChainId) -> FeeTokenAddresses {
    match chain_id {
        // Mainnet, testnet and integration systems have the same fee token addresses.
        ChainId::Mainnet | ChainId::Sepolia | ChainId::IntegrationSepolia => FeeTokenAddresses {
            strk_fee_token_address: *STRK_FEE_CONTRACT_ADDRESS,
            eth_fee_token_address: *ETH_FEE_CONTRACT_ADDRESS,
        },
        unknown_chain => unimplemented!("Unknown chain ID {unknown_chain}."),
    }
}

/// Returns the RPC state reader configuration with the constant RPC_NODE_URL.
pub fn get_rpc_state_reader_config() -> RpcStateReaderConfig {
    RpcStateReaderConfig::from_url(RPC_NODE_URL.clone())
}

/// Returns the chain info of mainnet.
pub fn get_chain_info(chain_id: &ChainId) -> ChainInfo {
    ChainInfo {
        chain_id: chain_id.clone(),
        fee_token_addresses: get_fee_token_addresses(chain_id),
        is_l3: false,
    }
}

// TODO(Aner): import the following functions instead, to reduce code duplication.
pub(crate) fn disjoint_hashmap_union<K: std::hash::Hash + std::cmp::Eq, V>(
    map1: IndexMap<K, V>,
    map2: IndexMap<K, V>,
) -> IndexMap<K, V> {
    let expected_len = map1.len() + map2.len();
    let union_map: IndexMap<K, V> = map1.into_iter().chain(map2).collect();
    // verify union length is sum of lengths (disjoint union)
    assert_eq!(union_map.len(), expected_len, "Intersection of hashmaps is not empty.");
    union_map
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReexecutionStateMaps {
    nonces: HashMap<ContractAddress, Nonce>,
    class_hashes: HashMap<ContractAddress, ClassHash>,
    storage: HashMap<ContractAddress, HashMap<StorageKey, Felt>>,
    compiled_class_hashes: HashMap<ClassHash, CompiledClassHash>,
    declared_contracts: HashMap<ClassHash, bool>,
}

impl From<StateMaps> for ReexecutionStateMaps {
    fn from(value: StateMaps) -> Self {
        let mut storage: HashMap<ContractAddress, HashMap<StorageKey, Felt>> = HashMap::new();
        for ((address, key), v) in value.storage {
            match storage.get_mut(&address) {
                Some(entry) => {
                    entry.insert(key, v);
                }
                None => {
                    let mut entry = HashMap::new();
                    entry.insert(key, v);
                    storage.insert(address, entry);
                }
            }
        }
        ReexecutionStateMaps {
            nonces: value.nonces,
            class_hashes: value.class_hashes,
            storage,
            compiled_class_hashes: value.compiled_class_hashes,
            declared_contracts: value.declared_contracts,
        }
    }
}

impl TryFrom<ReexecutionStateMaps> for StateMaps {
    type Error = ReexecutionError;

    fn try_from(value: ReexecutionStateMaps) -> Result<Self, Self::Error> {
        let mut storage: HashMap<(ContractAddress, StorageKey), Felt> = HashMap::new();
        for (address, inner_map) in value.storage {
            for (key, v) in inner_map {
                storage.insert((address, key), v);
            }
        }
        Ok(StateMaps {
            nonces: value.nonces,
            class_hashes: value.class_hashes,
            storage,
            compiled_class_hashes: value.compiled_class_hashes,
            declared_contracts: value.declared_contracts,
        })
    }
}

#[macro_export]
macro_rules! retry_request {
    ($retry_config:expr, $closure:expr) => {{
        let mut attempt_number = 0;
        retry::retry(
            retry::delay::Fixed::from_millis($retry_config.retry_interval_milliseconds)
                .take($retry_config.n_retries),
            || {
                attempt_number += 1;
                match $closure() {
                    Ok(value) => retry::OperationResult::Ok(value),
                    // If the error contains any of the expected error strings, we want to retry.
                    Err(e)
                        if $retry_config
                            .expected_error_strings
                            .iter()
                            .any(|s| e.to_string().contains(s)) =>
                    {
                        println!(
                            "Attempt {}: Retrying request due to error: {:?}",
                            attempt_number, e
                        );
                        println!(
                            "Retry delay in milliseconds: {}",
                            $retry_config.retry_interval_milliseconds
                        );
                        retry::OperationResult::Retry(e)
                    }
                    // For all other errors, do not retry and return immediately.
                    Err(e) => retry::OperationResult::Err(e),
                }
            },
        )
        .map_err(|e| {
            if $retry_config.expected_error_strings.iter().any(|s| e.error.to_string().contains(s))
            {
                panic!("{}: {:?}", $retry_config.retry_failure_message, e.error);
            }
            e.error
        })
    }};
}

/// A struct for comparing `CommitmentStateDiff` instances, disregarding insertion order.
/// This struct converts `IndexMap` fields to `BTreeMap`, providing a consistent ordering of keys.
/// This allows `assert_eq` to produce clearer, ordered diffs when comparing instances, especially
/// useful in testing.
#[derive(Debug, PartialEq)]
pub struct ComparableStateDiff {
    address_to_class_hash: BTreeMap<ContractAddress, ClassHash>,
    address_to_nonce: BTreeMap<ContractAddress, Nonce>,
    storage_updates: BTreeMap<ContractAddress, BTreeMap<StorageKey, Felt>>,
    class_hash_to_compiled_class_hash: BTreeMap<ClassHash, CompiledClassHash>,
}

impl From<CommitmentStateDiff> for ComparableStateDiff {
    fn from(state_diff: CommitmentStateDiff) -> Self {
        // Use helper function to convert IndexMap to HashMap for simplicity
        fn to_btree_map<K: std::cmp::Ord, V>(index_map: IndexMap<K, V>) -> BTreeMap<K, V> {
            index_map.into_iter().collect()
        }

        ComparableStateDiff {
            address_to_class_hash: to_btree_map(state_diff.address_to_class_hash),
            address_to_nonce: to_btree_map(state_diff.address_to_nonce),
            storage_updates: state_diff
                .storage_updates
                .into_iter()
                .map(|(address, storage)| (address, to_btree_map(storage)))
                .collect(),
            class_hash_to_compiled_class_hash: to_btree_map(
                state_diff.class_hash_to_compiled_class_hash,
            ),
        }
    }
}

pub fn reexecute_block_for_testing(block_number: u64) {
    // In tests we are already in the blockifier_reexecution directory.
    let full_file_path = format!("./resources/block_{block_number}/reexecution_data.json");

    // Initialize the contract class manager.
    let mut contract_class_manager_config = ContractClassManagerConfig::default();
    if cfg!(feature = "cairo_native") {
        contract_class_manager_config.cairo_native_run_config.wait_on_native_compilation = true;
        contract_class_manager_config.cairo_native_run_config.run_cairo_native = true;
    }
    let contract_class_manager = ContractClassManager::start(contract_class_manager_config);

    OfflineConsecutiveStateReaders::new_from_file(&full_file_path, contract_class_manager)
        .unwrap()
        .reexecute_and_verify_correctness();

    println!("Reexecution test for block {block_number} passed successfully.");
}

pub fn write_block_reexecution_data_to_file(
    block_number: BlockNumber,
    full_file_path: String,
    node_url: String,
    chain_id: ChainId,
    contract_class_manager: ContractClassManager,
) {
    let config = RpcStateReaderConfig::from_url(node_url);

    let consecutive_state_readers = ConsecutiveTestStateReaders::new(
        block_number.prev().expect("Should not run with block 0"),
        Some(config),
        chain_id.clone(),
        true,
        contract_class_manager,
    );

    let serializable_data_next_block =
        consecutive_state_readers.get_serializable_data_next_block().unwrap();

    let old_block_hash = consecutive_state_readers.get_old_block_hash().unwrap();

    // Run the reexecution and get the state maps and contract class mapping.
    let (block_state, expected_state_diff, actual_state_diff) =
        consecutive_state_readers.reexecute_block();

    // Warn if state diffs don't match, but continue writing the file.
    let expected_comparable = ComparableStateDiff::from(expected_state_diff);
    let actual_comparable = ComparableStateDiff::from(actual_state_diff);
    if expected_comparable != actual_comparable {
        println!(
            "WARNING: State diff mismatch for block {block_number}. Expected and actual state \
             diffs do not match."
        );
    }

    let block_state = block_state.unwrap();
    let serializable_data_prev_block = SerializableDataPrevBlock {
        state_maps: block_state.get_initial_reads().unwrap().into(),
        contract_class_mapping: block_state
            .state
            .state_reader
            .get_contract_class_mapping_dumper()
            .unwrap(),
    };

    // Write the reexecution data to a json file.
    SerializableOfflineReexecutionData {
        serializable_data_prev_block,
        serializable_data_next_block,
        chain_id,
        old_block_hash,
    }
    .write_to_file(&full_file_path)
    .unwrap();

    println!("RPC replies required for reexecuting block {block_number} written to json file.");
}

/// Executes a single transaction from a JSON file or given a transaction hash, using RPC to fetch
/// block context. Does not assert correctness, only prints the execution result.
pub fn execute_single_transaction(
    block_number: BlockNumber,
    node_url: String,
    chain_id: ChainId,
    tx_input: TransactionInput,
    contract_class_manager: ContractClassManager,
) -> ReexecutionResult<()> {
    // Create RPC config.
    let config = RpcStateReaderConfig::from_url(node_url);

    // Create ConsecutiveTestStateReaders first.
    let consecutive_state_readers = ConsecutiveTestStateReaders::new(
        block_number.prev().expect("Should not run with block 0"),
        Some(config),
        chain_id.clone(),
        false, // dump_mode = false
        contract_class_manager,
    );

    // Get transaction and hash based on input method.
    let (transaction, transaction_hash) = match tx_input {
        TransactionInput::FromHash { tx_hash } => {
            // Fetch transaction from the next block (the block containing the transaction to
            // execute).
            let transaction =
                consecutive_state_readers.next_block_state_reader.get_tx_by_hash(&tx_hash)?;
            let transaction_hash = TransactionHash(Felt::from_hex_unchecked(&tx_hash));

            (transaction, transaction_hash)
        }
        TransactionInput::FromFile { tx_path } => {
            // Load the transaction from a local JSON file.
            let json_content = read_to_string(&tx_path)
                .unwrap_or_else(|_| panic!("Failed to read transaction JSON file: {}.", tx_path));
            let json_value: Value = serde_json::from_str(&json_content)?;
            let transaction = deserialize_transaction_json_to_starknet_api_tx(json_value)?;
            let transaction_hash = transaction.calculate_transaction_hash(&chain_id)?;

            (transaction, transaction_hash)
        }
    };

    // Convert transaction to BlockifierTransaction using api_txs_to_blockifier_txs_next_block.
    let blockifier_tx = consecutive_state_readers
        .next_block_state_reader
        .api_txs_to_blockifier_txs_next_block(vec![(transaction, transaction_hash)])?;

    // Create transaction executor.
    let mut transaction_executor =
        consecutive_state_readers.pre_process_and_create_executor(None)?;

    // Execute transaction (should be single element).
    let execution_results = transaction_executor.execute_txs(&blockifier_tx, None);

    // We expect exactly one execution result since we executed a single transaction.
    let res =
        execution_results.first().expect("Expected exactly one execution result, but got none");

    println!("Execution result: {:?}", res);

    Ok(())
}

/// Asserts equality between two `CommitmentStateDiff` structs, ignoring insertion order.
#[macro_export]
macro_rules! assert_eq_state_diff {
    ($expected_state_diff:expr, $actual_state_diff:expr $(,)?) => {
        pretty_assertions::assert_eq!(
            $crate::state_reader::utils::ComparableStateDiff::from($expected_state_diff,),
            $crate::state_reader::utils::ComparableStateDiff::from($actual_state_diff,),
            "Expected and actual state diffs do not match.",
        );
    };
}
