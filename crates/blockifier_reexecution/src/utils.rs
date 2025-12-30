use std::collections::{BTreeMap, HashMap};
use std::env;
use std::sync::{Arc, LazyLock};

use apollo_gateway_config::config::RpcStateReaderConfig;
use apollo_rpc_execution::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use blockifier::context::{ChainInfo, FeeTokenAddresses};
use blockifier::execution::contract_class::{CompiledClassV0, CompiledClassV1};
use blockifier::state::cached_state::{CommitmentStateDiff, StateMaps};
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::StateResult;
use indexmap::IndexMap;
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_types_core::felt::Felt;

use crate::errors::ReexecutionError;

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
    ($retry_config:expr_2021, $closure:expr_2021) => {{
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

/// Asserts equality between two `CommitmentStateDiff` structs, ignoring insertion order.
#[macro_export]
macro_rules! assert_eq_state_diff {
    ($expected_state_diff:expr_2021, $actual_state_diff:expr_2021 $(,)?) => {
        pretty_assertions::assert_eq!(
            $crate::utils::ComparableStateDiff::from($expected_state_diff,),
            $crate::utils::ComparableStateDiff::from($actual_state_diff,),
            "Expected and actual state diffs do not match.",
        );
    };
}
