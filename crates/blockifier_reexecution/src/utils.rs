use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write;
use std::sync::{Arc, LazyLock};

use apollo_rpc_execution::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use blockifier::context::{ChainInfo, FeeTokenAddresses};
use blockifier::execution::contract_class::{CompiledClassV0, CompiledClassV1};
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::StateResult;
use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::state::SierraContractClass;

use crate::state_reader::config::RpcStateReaderConfig;

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

/// Compares two state diffs. Returns `true` if they match. On mismatch, logs a detailed diff.
pub fn compare_state_diffs(
    expected_state_diff: CommitmentStateDiff,
    actual_state_diff: CommitmentStateDiff,
    block_number: BlockNumber,
) -> bool {
    match format_state_diff_mismatch(expected_state_diff, actual_state_diff) {
        None => true,
        Some(diff) => {
            tracing::warn!("State diff mismatch for block {block_number}.\n{diff}");
            false
        }
    }
}

/// Returns `None` if the two state diffs match, or `Some(diff)` with a human-readable description
/// of the differences.
pub(crate) fn format_state_diff_mismatch(
    expected_state_diff: CommitmentStateDiff,
    actual_state_diff: CommitmentStateDiff,
) -> Option<String> {
    if expected_state_diff == actual_state_diff {
        return None;
    }
    // Destructuring `CommitmentStateDiff` ensures a compile error if new fields are added without
    // updating this function.
    let CommitmentStateDiff {
        address_to_class_hash: expected_class_hashes,
        address_to_nonce: expected_nonces,
        storage_updates: expected_storage,
        class_hash_to_compiled_class_hash: expected_compiled_class_hashes,
    } = expected_state_diff;
    let CommitmentStateDiff {
        address_to_class_hash: actual_class_hashes,
        address_to_nonce: actual_nonces,
        storage_updates: actual_storage,
        class_hash_to_compiled_class_hash: actual_compiled_class_hashes,
    } = actual_state_diff;

    let mut output = String::new();
    diff_flat_map("address_to_class_hash", expected_class_hashes, actual_class_hashes, &mut output);
    diff_flat_map("address_to_nonce", expected_nonces, actual_nonces, &mut output);
    diff_flat_map(
        "class_hash_to_compiled_class_hash",
        expected_compiled_class_hashes,
        actual_compiled_class_hashes,
        &mut output,
    );

    // storage_updates is a nested map — diff each contract's storage separately.
    // Convert only the outer map to BTreeMap for sorted address iteration;
    // inner IndexMaps are passed directly to diff_flat_map.
    let mut expected_storage: BTreeMap<_, _> = expected_storage.into_iter().collect();
    let mut actual_storage: BTreeMap<_, _> = actual_storage.into_iter().collect();
    let all_addresses: BTreeSet<ContractAddress> =
        expected_storage.keys().chain(actual_storage.keys()).cloned().collect();
    for address in all_addresses {
        match (expected_storage.remove(&address), actual_storage.remove(&address)) {
            (Some(expected_keys), Some(actual_keys)) => {
                diff_flat_map(
                    &format!("storage_updates[{address}]"),
                    expected_keys,
                    actual_keys,
                    &mut output,
                );
            }
            (Some(expected_keys), None) => {
                writeln!(
                    output,
                    "  storage_updates[{address}]: missing in actual (expected {expected_keys:?})"
                )
                .unwrap();
            }
            (None, Some(actual_keys)) => {
                writeln!(
                    output,
                    "  storage_updates[{address}]: missing in expected (actual {actual_keys:?})"
                )
                .unwrap();
            }
            _ => {}
        }
    }

    Some(output)
}

/// Appends mismatching entries between two maps to `output`, under a named section header.
fn diff_flat_map<K, V>(
    name: &str,
    expected: IndexMap<K, V>,
    actual: IndexMap<K, V>,
    output: &mut String,
) where
    K: Ord + std::hash::Hash + Eq + std::fmt::Debug,
    V: PartialEq + std::fmt::Debug,
{
    let expected: BTreeMap<K, V> = expected.into_iter().collect();
    let actual: BTreeMap<K, V> = actual.into_iter().collect();
    let all_keys: BTreeSet<&K> = expected.keys().chain(actual.keys()).collect();
    let mut section_printed = false;
    for key in all_keys {
        let (expected_value, actual_value) = (expected.get(key), actual.get(key));
        let is_diff = match (&expected_value, &actual_value) {
            (Some(expected_value), Some(actual_value)) => expected_value != actual_value,
            (Some(_), None) | (None, Some(_)) => true,
            _ => false,
        };
        if !is_diff {
            continue;
        }
        if !section_printed {
            writeln!(output, "  {name}:").unwrap();
            section_printed = true;
        }
        writeln!(output, "    {key:?}:").unwrap();
        match (expected_value, actual_value) {
            (Some(expected_value), Some(actual_value)) => {
                writeln!(output, "      expected: {expected_value:?}").unwrap();
                writeln!(output, "      actual:   {actual_value:?}").unwrap();
            }
            (Some(expected_value), None) => {
                writeln!(output, "      expected: {expected_value:?}").unwrap();
                writeln!(output, "      actual:   (missing)").unwrap();
            }
            (None, Some(actual_value)) => {
                writeln!(output, "      expected: (missing)").unwrap();
                writeln!(output, "      actual:   {actual_value:?}").unwrap();
            }
            _ => {}
        }
    }
}

/// Asserts equality between two `CommitmentStateDiff` structs, ignoring insertion order.
/// On failure, panics with a human-readable diff.
#[macro_export]
macro_rules! assert_eq_state_diff {
    ($expected_state_diff:expr, $actual_state_diff:expr $(,)?) => {
        if let Some(diff) =
            $crate::utils::format_state_diff_mismatch($expected_state_diff, $actual_state_diff)
        {
            panic!("State diffs do not match.\n{diff}");
        }
    };
}
