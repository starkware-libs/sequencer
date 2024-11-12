use std::collections::{BTreeMap, HashMap};

use assert_matches::assert_matches;
use blockifier::context::{ChainInfo, FeeTokenAddresses};
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff, StateMaps};
use blockifier::state::state_api::StateReader;
use indexmap::IndexMap;
use papyrus_execution::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_api::test_utils::read_json_file;
use starknet_gateway::config::RpcStateReaderConfig;
use starknet_types_core::felt::Felt;

use crate::assert_eq_state_diff;
use crate::state_reader::errors::ReexecutionError;
use crate::state_reader::test_state_reader::{
    ConsecutiveStateReaders,
    ConsecutiveTestStateReaders,
    OfflineConsecutiveStateReaders,
    SerializableDataPrevBlock,
    SerializableOfflineReexecutionData,
};

pub const RPC_NODE_URL: &str = "https://free-rpc.nethermind.io/mainnet-juno/";
pub const JSON_RPC_VERSION: &str = "2.0";

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

/// Returns the RPC state reader configuration with the constants RPC_NODE_URL and JSON_RPC_VERSION.
pub fn get_rpc_state_reader_config() -> RpcStateReaderConfig {
    RpcStateReaderConfig {
        url: RPC_NODE_URL.to_string(),
        json_rpc_version: JSON_RPC_VERSION.to_string(),
    }
}

/// Returns the chain info of mainnet.
pub fn get_chain_info(chain_id: &ChainId) -> ChainInfo {
    ChainInfo { chain_id: chain_id.clone(), fee_token_addresses: get_fee_token_addresses(chain_id) }
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

pub fn reexecute_and_verify_correctness<
    S: StateReader + Send + Sync,
    T: ConsecutiveStateReaders<S>,
>(
    consecutive_state_readers: T,
) -> Option<CachedState<S>> {
    let expected_state_diff = consecutive_state_readers.get_next_block_state_diff().unwrap();

    let all_txs_in_next_block = consecutive_state_readers.get_next_block_txs().unwrap();

    let mut transaction_executor =
        consecutive_state_readers.pre_process_and_create_executor(None).unwrap();

    let execution_results = transaction_executor.execute_txs(&all_txs_in_next_block);
    // Verify all transactions executed successfully.
    for res in execution_results.iter() {
        assert_matches!(res, Ok(_));
    }

    // Finalize block and read actual statediff.
    let (actual_state_diff, _, _) =
        transaction_executor.finalize().expect("Couldn't finalize block");

    assert_eq_state_diff!(expected_state_diff, actual_state_diff);

    transaction_executor.block_state
}

pub fn reexecute_block_for_testing(block_number: u64) {
    // In tests we are already in the blockifier_reexecution directory.
    let full_file_path = format!("./resources/block_{block_number}/reexecution_data.json");

    reexecute_and_verify_correctness(
        OfflineConsecutiveStateReaders::new_from_file(&full_file_path).unwrap(),
    );

    println!("Reexecution test for block {block_number} passed successfully.");
}

pub fn write_block_reexecution_data_to_file(
    block_number: BlockNumber,
    full_file_path: &str,
    node_url: String,
    chain_id: ChainId,
) {
    let config =
        RpcStateReaderConfig { url: node_url, json_rpc_version: JSON_RPC_VERSION.to_string() };

    let consecutive_state_readers = ConsecutiveTestStateReaders::new(
        block_number.prev().expect("Should not run with block 0"),
        Some(config),
        chain_id.clone(),
        true,
    );

    let serializable_data_next_block =
        consecutive_state_readers.get_serializable_data_next_block().unwrap();

    let old_block_hash = consecutive_state_readers.get_old_block_hash().unwrap();

    // Run the reexecution test and get the state maps and contract class mapping.
    let block_state = reexecute_and_verify_correctness(consecutive_state_readers).unwrap();
    let serializable_data_prev_block = SerializableDataPrevBlock {
        state_maps: block_state.get_initial_reads().unwrap().into(),
        contract_class_mapping: block_state.state.get_contract_class_mapping_dumper().unwrap(),
    };

    // Write the reexecution data to a json file.
    SerializableOfflineReexecutionData {
        serializable_data_prev_block,
        serializable_data_next_block,
        chain_id,
        old_block_hash,
    }
    .write_to_file(full_file_path)
    .unwrap();

    println!("RPC replies required for reexecuting block {block_number} written to json file.");
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

/// Returns the block numbers for re-execution.
/// There is block number for each Starknet Version (starting v0.13)
/// And some additional block with specific transactions.
pub fn get_block_numbers_for_reexecution() -> Vec<BlockNumber> {
    let block_numbers_examples: HashMap<String, u64> =
        serde_json::from_value(read_json_file("block_numbers_for_reexecution.json"))
            .expect("Failed to deserialize block header");
    block_numbers_examples.values().cloned().map(BlockNumber).collect()
}
