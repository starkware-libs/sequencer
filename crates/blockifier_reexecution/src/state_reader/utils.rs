use std::collections::{BTreeMap, HashMap};

use blockifier::context::{ChainInfo, FeeTokenAddresses};
use blockifier::state::cached_state::{CommitmentStateDiff, StateMaps};
use indexmap::IndexMap;
use papyrus_execution::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_gateway::config::RpcStateReaderConfig;
use starknet_types_core::felt::Felt;

use crate::state_reader::errors::ReexecutionError;

pub const RPC_NODE_URL: &str = "https://free-rpc.nethermind.io/mainnet-juno/";
pub const JSON_RPC_VERSION: &str = "2.0";

/// Returns the fee token addresses of mainnet.
pub fn get_fee_token_addresses() -> FeeTokenAddresses {
    FeeTokenAddresses {
        strk_fee_token_address: *STRK_FEE_CONTRACT_ADDRESS,
        eth_fee_token_address: *ETH_FEE_CONTRACT_ADDRESS,
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
pub fn get_chain_info() -> ChainInfo {
    ChainInfo { chain_id: ChainId::Mainnet, fee_token_addresses: get_fee_token_addresses() }
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
pub(crate) struct ReexecutionStateMaps {
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
        retry::retry(
            retry::delay::Fixed::from_millis($retry_config.retry_interval_milliseconds)
                .take($retry_config.n_retries),
            || {
                match $closure() {
                    Ok(value) => retry::OperationResult::Ok(value),
                    // If the error contains the expected_error_string , we want to retry.
                    Err(e) if e.to_string().contains($retry_config.expected_error_string) => {
                        retry::OperationResult::Retry(e)
                    }
                    // For all other errors, do not retry and return immediately.
                    Err(e) => retry::OperationResult::Err(e),
                }
            },
        )
        .map_err(|e| {
            if e.error.to_string().contains($retry_config.expected_error_string) {
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

/// Asserts equality between two `CommitmentStateDiff` structs, ignoring insertion order.
#[macro_export]
macro_rules! assert_eq_state_diff {
    ($expected_state_diff:expr, $actual_state_diff:expr $(,)?) => {
        use blockifier_reexecution::state_reader::utils::ComparableStateDiff;
        use pretty_assertions::assert_eq;

        assert_eq!(
            ComparableStateDiff::from($expected_state_diff),
            ComparableStateDiff::from($actual_state_diff),
            "Expected and actual state diffs do not match."
        );
    };
}
