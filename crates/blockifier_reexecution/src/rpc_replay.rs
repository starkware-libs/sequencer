use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "cairo_native")]
use blockifier::blockifier::config::{CairoNativeMode, ContractClassManagerConfig};
use blockifier::context::ChainInfo;
use blockifier::state::contract_class_manager::ContractClassManager;
use starknet_api::block::{BlockInfo, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_commitments,
    calculate_block_hash,
    PartialBlockHashComponents,
    TransactionHashingData,
};
#[cfg(feature = "cairo_native")]
use starknet_api::contract_class::SierraVersion;
use starknet_api::state::ThinStateDiff;

use crate::errors::{RPCStateReaderError, ReexecutionError, ReexecutionResult};
use crate::state_reader::config::RpcStateReaderConfig;
use crate::state_reader::reexecution_state_reader::ConsecutiveReexecutionStateReaders;
use crate::state_reader::rpc_objects::BlockHeader;
use crate::state_reader::rpc_state_reader::ConsecutiveRpcStateReaders;
use crate::utils::{compare_state_diffs, get_chain_info};
// Block hash comparison is only valid for Starknet v0.14.0 and later.
// Earlier blocks may have deprecated (Cairo 0) declared classes which we do not fetch from RPC,
// and would cause the computed hash to diverge from the chain hash.
const MIN_VERSION_FOR_BLOCK_HASH_COMPARISON: &str = "0.14.0";

struct ReplayCounters {
    matched: AtomicU64,
    failed: AtomicU64,
    mismatched: AtomicU64,
}

/// Runs continuous RPC replay from `start_block` to `end_block` (or forever if `None`),
/// using `n_workers` worker threads. Each block is reexecuted and the resulting state diff
/// is compared against the expected one from the chain.
pub async fn run_rpc_replay(
    node_url: String,
    chain_id: starknet_api::core::ChainId,
    start_block: u64,
    end_block: Option<u64>,
    n_workers: usize,
    contract_class_manager: ContractClassManager,
    compare_native: bool,
) {
    let rpc_state_reader_config = RpcStateReaderConfig::from_url(node_url);
    let chain_info = get_chain_info(&chain_id, None);
    let block_counter = Arc::new(AtomicU64::new(start_block));
    let counters = Arc::new(ReplayCounters {
        matched: AtomicU64::new(0),
        failed: AtomicU64::new(0),
        mismatched: AtomicU64::new(0),
    });

    let end_block_str = end_block.map_or("inf".to_string(), |b| b.to_string());
    tracing::info!(
        "Starting RPC replay from block {start_block} to {end_block_str} with {n_workers} \
         workers, compare_native={compare_native}."
    );

    let replay_mode = match compare_native {
        true => {
            #[cfg(not(feature = "cairo_native"))]
            panic!("--compare-native requires the cairo_native feature");

            #[cfg(feature = "cairo_native")]
            {
                let mut native_config = ContractClassManagerConfig::default();
                native_config.cairo_native_run_config.cairo_native_mode =
                    CairoNativeMode::WaitOnCompilation;
                let mut casm_config = ContractClassManagerConfig::default();
                casm_config.cairo_native_run_config.cairo_native_mode = CairoNativeMode::Off;
                ReplayMode::CompareNative {
                    native_manager: ContractClassManager::start(native_config),
                    casm_manager: ContractClassManager::start(casm_config),
                }
            }
        }
        false => ReplayMode::Standard { contract_class_manager },
    };

    let mut task_set = tokio::task::JoinSet::new();
    for _ in 0..n_workers {
        let counter = block_counter.clone();
        let config = rpc_state_reader_config.clone();
        let chain_info = chain_info.clone();
        let replay_mode = replay_mode.clone();
        let counters = counters.clone();

        task_set.spawn_blocking(move || {
            replay_worker(counter, end_block, &config, &chain_info, &replay_mode, &counters);
        });
    }
    task_set.join_all().await;

    let matched = counters.matched.load(Ordering::Relaxed);
    let mismatched = counters.mismatched.load(Ordering::Relaxed);
    let failed = counters.failed.load(Ordering::Relaxed);
    let total = matched + mismatched + failed;
    tracing::info!(
        "RPC replay complete. Total: {total}, matched: {matched}, mismatched: {mismatched}, \
         failed: {failed}."
    );
}

#[derive(Clone)]
enum ReplayMode {
    Standard {
        contract_class_manager: ContractClassManager,
    },
    #[cfg(feature = "cairo_native")]
    CompareNative {
        native_manager: ContractClassManager,
        casm_manager: ContractClassManager,
    },
}

fn replay_worker(
    block_counter: Arc<AtomicU64>,
    end_block: Option<u64>,
    config: &RpcStateReaderConfig,
    chain_info: &ChainInfo,
    replay_mode: &ReplayMode,
    counters: &ReplayCounters,
) {
    loop {
        // If `end_block` is not `None`, we need to check if we've reached it.
        let block_number = block_counter.fetch_add(1, Ordering::Relaxed);
        if let Some(end) = end_block {
            if block_number > end {
                break;
            }
        }

        // Retry until the block is available on the node. If the node hasn't yet reached
        // this block number (i.e., we're ahead of the chain tip), we wait and retry.
        let result = loop {
            let attempt = match replay_mode {
                ReplayMode::Standard { contract_class_manager } => {
                    reexecute_block(block_number, config, chain_info, contract_class_manager)
                }
                #[cfg(feature = "cairo_native")]
                ReplayMode::CompareNative { native_manager, casm_manager } => {
                    reexecute_block_native_vs_casm(
                        block_number,
                        config,
                        chain_info,
                        native_manager,
                        casm_manager,
                    )
                }
            };
            match attempt {
                Err(ref e) if is_block_not_found(e) => {
                    tracing::debug!("Block {block_number} not found, waiting for chain tip.");
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                result => break result,
            }
        };

        match result {
            Ok(true) => {
                counters.matched.fetch_add(1, Ordering::Relaxed);
                tracing::info!("Block {block_number} matched.");
            }
            Ok(false) => {
                counters.mismatched.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                counters.failed.fetch_add(1, Ordering::Relaxed);
                tracing::error!("Block {block_number} reexecution failed: {e}");
            }
        }
    }
}

/// Reexecutes a single block via RPC, compares the state diff and block hash against the chain.
fn reexecute_block(
    block_number: u64,
    config: &RpcStateReaderConfig,
    chain_info: &ChainInfo,
    contract_class_manager: &ContractClassManager,
) -> ReexecutionResult<bool> {
    let prev_block = BlockNumber(block_number)
        .prev()
        .expect("Block number 0 cannot be reexecuted (no previous block).");
    let readers = ConsecutiveRpcStateReaders::new(
        prev_block,
        Some(config.clone()),
        chain_info.clone(),
        false,
        contract_class_manager.clone(),
    );

    let block_header = readers.get_next_block_header()?;

    let (_block_state, expected_state_diff, actual_state_diff, txs_hashing_data) =
        readers.reexecute_block()?;

    let state_diff_matched = compare_state_diffs(
        expected_state_diff,
        actual_state_diff.clone(),
        BlockNumber(block_number),
    );
    let block_hash_matched = compare_block_hash(
        txs_hashing_data,
        actual_state_diff,
        &block_header,
        BlockNumber(block_number),
    );

    Ok(state_diff_matched && block_hash_matched)
}

/// Reexecutes a single block twice -- once with native and once with CASM -- and compares the
/// resulting state diffs and transaction hashing data against each other.
///
/// Comparing transaction hashing data (execution outputs, signatures) is equivalent to comparing
/// block hashes without the overhead of computing commitments.
#[cfg(feature = "cairo_native")]
fn reexecute_block_native_vs_casm(
    block_number: u64,
    config: &RpcStateReaderConfig,
    chain_info: &ChainInfo,
    native_manager: &ContractClassManager,
    casm_manager: &ContractClassManager,
) -> ReexecutionResult<bool> {
    let prev_block = BlockNumber(block_number)
        .prev()
        .expect("Block number 0 cannot be reexecuted (no previous block).");

    let min_sierra_version_override = Some(SierraVersion::new(0, 0, 0));

    let mut native_readers = ConsecutiveRpcStateReaders::new(
        prev_block,
        Some(config.clone()),
        chain_info.clone(),
        false,
        native_manager.clone(),
    );
    native_readers.min_sierra_version_override = min_sierra_version_override.clone();
    let (_block_state, _expected, native_state_diff, native_txs_hashing_data) =
        native_readers.reexecute_block()?;

    let mut casm_readers = ConsecutiveRpcStateReaders::new(
        prev_block,
        Some(config.clone()),
        chain_info.clone(),
        false,
        casm_manager.clone(),
    );
    casm_readers.min_sierra_version_override = min_sierra_version_override;
    let (_block_state, _expected, casm_state_diff, casm_txs_hashing_data) =
        casm_readers.reexecute_block()?;

    let state_diff_matched =
        compare_state_diffs(native_state_diff, casm_state_diff, BlockNumber(block_number));
    let tx_hashing_data_matched =
        compare_tx_hashing_data(native_txs_hashing_data, casm_txs_hashing_data, block_number);

    Ok(state_diff_matched && tx_hashing_data_matched)
}

/// Computes the block hash from the reexecution output and compares it against the expected hash
/// from the chain. Returns `true` if they match, or if the block predates v0.14.0 (skipped).
///
/// Uses the state root from the RPC block header (`new_root`) since the blockifier does not
/// compute state roots. If the state diff already matched, the state root should also match.
///
/// Note: We assume no deprecated (Cairo 0) declared classes in any block. This assumption holds
/// for Starknet v0.14.0+. Blocks before v0.14.0 may include such declarations and are therefore
/// skipped.
fn compare_block_hash(
    txs_hashing_data: Vec<TransactionHashingData>,
    actual_state_diff: blockifier::state::cached_state::CommitmentStateDiff,
    block_header: &BlockHeader,
    block_number: BlockNumber,
) -> bool {
    let starknet_version: starknet_api::block::StarknetVersion =
        block_header.starknet_version.clone().try_into().expect("Failed to parse starknet version");

    let min_version: starknet_api::block::StarknetVersion =
        MIN_VERSION_FOR_BLOCK_HASH_COMPARISON.try_into().expect("Invalid min version constant.");
    if starknet_version < min_version {
        tracing::debug!(
            "Block {block_number}: skipping block hash comparison (version {} < {}).",
            block_header.starknet_version,
            MIN_VERSION_FOR_BLOCK_HASH_COMPARISON
        );
        return true;
    }

    let thin_state_diff = ThinStateDiff {
        deployed_contracts: actual_state_diff.address_to_class_hash,
        storage_diffs: actual_state_diff.storage_updates,
        class_hash_to_compiled_class_hash: actual_state_diff.class_hash_to_compiled_class_hash,
        deprecated_declared_classes: vec![],
        nonces: actual_state_diff.address_to_nonce,
    };

    let (commitments, _measurements) =
        tokio::runtime::Handle::current().block_on(calculate_block_commitments(
            &txs_hashing_data,
            thin_state_diff,
            block_header.l1_da_mode,
            &starknet_version,
        ));

    let block_info: BlockInfo = block_header.clone().try_into().expect("Invalid block header");
    let partial_block_hash_components = PartialBlockHashComponents::new(&block_info, commitments);

    let computed_hash = calculate_block_hash(
        &partial_block_hash_components,
        block_header.new_root,
        block_header.parent_hash,
    )
    .expect("Failed to calculate block hash");

    if computed_hash == block_header.block_hash {
        true
    } else {
        tracing::warn!(
            "Block hash mismatch for block {block_number}.\n  expected: {}\n  actual:   {}",
            block_header.block_hash,
            computed_hash,
        );
        false
    }
}

/// Compares transaction hashing data (execution outputs and signatures) between two executions.
/// Returns `true` if they match.
#[cfg(feature = "cairo_native")]
fn compare_tx_hashing_data(
    native_txs_hashing_data: Vec<TransactionHashingData>,
    casm_txs_hashing_data: Vec<TransactionHashingData>,
    block_number: u64,
) -> bool {
    if native_txs_hashing_data == casm_txs_hashing_data {
        return true;
    }
    let native_len = native_txs_hashing_data.len();
    let casm_len = casm_txs_hashing_data.len();
    tracing::warn!(
        "Transaction hashing data mismatch for block {block_number} between native and CASM \
         (native: {native_len} txs, casm: {casm_len} txs)."
    );
    for (index, (native, casm)) in
        native_txs_hashing_data.iter().zip(casm_txs_hashing_data.iter()).enumerate()
    {
        if native != casm {
            tracing::warn!(
                "  tx[{index}] ({}):\n    native: {native:?}\n    casm:   {casm:?}",
                native.transaction_hash,
            );
        }
    }
    for index in casm_len..native_len {
        tracing::warn!(
            "  tx[{index}] ({}): present in native, missing in casm.",
            native_txs_hashing_data[index].transaction_hash,
        );
    }
    for index in native_len..casm_len {
        tracing::warn!(
            "  tx[{index}] ({}): missing in native, present in casm.",
            casm_txs_hashing_data[index].transaction_hash,
        );
    }
    false
}

fn is_block_not_found(err: &ReexecutionError) -> bool {
    matches!(err, ReexecutionError::Rpc(RPCStateReaderError::BlockNotFound(_)))
}
