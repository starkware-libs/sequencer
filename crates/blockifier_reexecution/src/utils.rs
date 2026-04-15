use std::collections::BTreeMap;
use std::env;
use std::sync::{Arc, LazyLock};

use apollo_rpc_execution::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use blockifier::context::{ChainInfo, FeeTokenAddresses};
use blockifier::execution::contract_class::{CompiledClassV0, CompiledClassV1};
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::StateResult;
use indexmap::IndexMap;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockInfo, BlockNumber, StarknetVersion};
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_commitments,
    calculate_block_hash,
    PartialBlockHashComponents,
    TransactionHashingData,
};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_types_core::felt::Felt;

use crate::errors::ReexecutionResult;
use crate::state_reader::config::RpcStateReaderConfig;
use crate::state_reader::rpc_objects::BlockHeader;

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

/// Returns the fee token addresses for the given chain.
/// If `strk_fee_token_address_override` is provided, it overrides the default STRK fee token
/// address.
pub fn get_fee_token_addresses(
    chain_id: &ChainId,
    strk_fee_token_address_override: Option<ContractAddress>,
) -> FeeTokenAddresses {
    match chain_id {
        // Mainnet, testnet and integration systems have the same fee token addresses.
        ChainId::Mainnet | ChainId::Sepolia | ChainId::IntegrationSepolia => FeeTokenAddresses {
            strk_fee_token_address: strk_fee_token_address_override
                .unwrap_or(*STRK_FEE_CONTRACT_ADDRESS),
            eth_fee_token_address: *ETH_FEE_CONTRACT_ADDRESS,
        },
        unknown_chain => unimplemented!("Unknown chain ID {unknown_chain}."),
    }
}

/// Returns the RPC state reader configuration with the constant RPC_NODE_URL.
pub fn get_rpc_state_reader_config() -> RpcStateReaderConfig {
    RpcStateReaderConfig::from_url(RPC_NODE_URL.clone())
}

/// Returns the chain info for the given chain.
pub fn get_chain_info(
    chain_id: &ChainId,
    strk_fee_token_address_override: Option<ContractAddress>,
) -> ChainInfo {
    ChainInfo {
        chain_id: chain_id.clone(),
        fee_token_addresses: get_fee_token_addresses(chain_id, strk_fee_token_address_override),
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
                        tracing::warn!(
                            "Attempt {attempt_number}: Retrying request due to error: {e:?}. \
                             Retry delay in milliseconds: {}",
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

/// Compares two state diffs, Returns `true` if they match.
/// On mismatch, logs a detailed diff.
pub fn compare_state_diffs(
    expected_state_diff: CommitmentStateDiff,
    actual_state_diff: CommitmentStateDiff,
    block_number: BlockNumber,
) -> bool {
    let expected = ComparableStateDiff::from(expected_state_diff);
    let actual = ComparableStateDiff::from(actual_state_diff);
    let is_match = expected == actual;
    if !is_match {
        let expected_str = format!("{expected:#?}");
        let actual_str = format!("{actual:#?}");
        let diff = pretty_assertions::StrComparison::new(&expected_str, &actual_str);
        tracing::warn!("State diff mismatch for block {block_number}.\n{diff}");
    }
    is_match
}

// Block hash comparison is only valid for Starknet v0.14.0 and later.
const MIN_VERSION_FOR_BLOCK_HASH_COMPARISON: &str = "0.14.0";

/// Computes the block hash from the reexecution output and compares it against the expected hash
/// from the chain. Returns `true` if they match, or if the block predates v0.14.0 (skipped).
///
/// Uses the state root from the RPC block header (`new_root`) since the blockifier does not
/// compute state roots. If the state diff already matched, the state root should also match.
///
/// Note: Blocks before v0.14.0 may include deprecated (Cairo 0) declared classes which are not
/// represented in [`CommitmentStateDiff`]; those blocks skip hash comparison below.
pub async fn compare_block_hash(
    txs_hashing_data: Vec<TransactionHashingData>,
    actual_state_diff: CommitmentStateDiff,
    block_header: &BlockHeader,
    block_number: BlockNumber,
) -> ReexecutionResult<bool> {
    let starknet_version: StarknetVersion = block_header.starknet_version.clone().try_into()?;

    let min_version: StarknetVersion =
        MIN_VERSION_FOR_BLOCK_HASH_COMPARISON.try_into().expect("Invalid min version constant.");
    if starknet_version < min_version {
        tracing::debug!(
            "Block {block_number}: skipping block hash comparison (version {} < {}).",
            block_header.starknet_version,
            MIN_VERSION_FOR_BLOCK_HASH_COMPARISON
        );
        return Ok(true);
    }

    let (commitments, _measurements) = calculate_block_commitments(
        &txs_hashing_data,
        actual_state_diff.into(),
        block_header.l1_da_mode,
        &starknet_version,
    )
    .await;

    let block_info: BlockInfo = block_header.clone().try_into()?;
    let partial_block_hash_components = PartialBlockHashComponents::new(&block_info, commitments);

    let computed_hash = calculate_block_hash(
        &partial_block_hash_components,
        block_header.new_root,
        block_header.parent_hash,
    )?;

    if computed_hash == block_header.block_hash {
        Ok(true)
    } else {
        tracing::warn!(
            "Block hash mismatch for block {block_number}.\n  expected: {}\n  actual:   {}",
            block_header.block_hash,
            computed_hash,
        );
        Ok(false)
    }
}

/// Asserts equality between two `CommitmentStateDiff` structs, ignoring insertion order.
#[macro_export]
macro_rules! assert_eq_state_diff {
    ($expected_state_diff:expr, $actual_state_diff:expr $(,)?) => {
        pretty_assertions::assert_eq!(
            $crate::utils::ComparableStateDiff::from($expected_state_diff,),
            $crate::utils::ComparableStateDiff::from($actual_state_diff,),
            "Expected and actual state diffs do not match.",
        );
    };
}
