use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "cairo_native")]
use blockifier::blockifier::config::{
    CairoNativeMode,
    CairoNativeRunConfig,
    ContractClassManagerConfig,
};
use blockifier::context::ChainInfo;
use blockifier::state::contract_class_manager::ContractClassManager;
use starknet_api::block::BlockNumber;
#[cfg(feature = "cairo_native")]
use starknet_api::contract_class::SierraVersion;

use crate::errors::{RPCStateReaderError, ReexecutionError, ReexecutionResult};
use crate::state_reader::config::RpcStateReaderConfig;
use crate::state_reader::reexecution_state_reader::{
    ConsecutiveReexecutionStateReaders,
    ReexecuteBlockOutcome,
};
use crate::state_reader::rpc_state_reader::ConsecutiveRpcStateReaders;
use crate::utils::{compare_state_diffs, get_chain_info};

struct ReplayCounters {
    matched: AtomicU64,
    failed: AtomicU64,
    mismatched: AtomicU64,
}

/// Runs continuous RPC replay from `start_block` to `end_block` (or forever if `None`),
/// using `n_workers` worker threads. Each block is reexecuted and the resulting state diff
/// is compared against the expected one from the chain.
#[allow(clippy::too_many_arguments)]
pub async fn run_rpc_replay(
    node_url: String,
    chain_id: starknet_api::core::ChainId,
    start_block: u64,
    end_block: Option<u64>,
    n_workers: usize,
    contract_class_manager: ContractClassManager,
    compare_native: bool,
    prefetch_initial_reads: bool,
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
                let native_config = ContractClassManagerConfig {
                    cairo_native_run_config: CairoNativeRunConfig::wait_on_compilation_for_testing(
                    ),
                    ..Default::default()
                };
                let casm_config = ContractClassManagerConfig {
                    cairo_native_run_config: CairoNativeRunConfig {
                        cairo_native_mode: CairoNativeMode::Off,
                        ..Default::default()
                    },
                    ..Default::default()
                };
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
            replay_worker(
                counter,
                end_block,
                &config,
                &chain_info,
                &replay_mode,
                &counters,
                prefetch_initial_reads,
            );
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
    prefetch_initial_reads: bool,
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
                ReplayMode::Standard { contract_class_manager } => reexecute_block(
                    block_number,
                    config,
                    chain_info,
                    contract_class_manager,
                    prefetch_initial_reads,
                ),
                #[cfg(feature = "cairo_native")]
                ReplayMode::CompareNative { native_manager, casm_manager } => {
                    reexecute_block_native_vs_casm(
                        block_number,
                        config,
                        chain_info,
                        native_manager,
                        casm_manager,
                        prefetch_initial_reads,
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

/// Reexecutes a single block via RPC and compares the actual state diff against the chain.
fn reexecute_block(
    block_number: u64,
    config: &RpcStateReaderConfig,
    chain_info: &ChainInfo,
    contract_class_manager: &ContractClassManager,
    prefetch_initial_reads: bool,
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
        prefetch_initial_reads,
    );

    let ReexecuteBlockOutcome { expected_state_diff, actual_state_diff, .. } =
        readers.reexecute_block()?;

    Ok(compare_state_diffs(expected_state_diff, actual_state_diff, BlockNumber(block_number)))
}

/// Reexecutes a single block twice -- once with native and once with CASM -- and compares the
/// resulting state diffs against each other.
#[cfg(feature = "cairo_native")]
fn reexecute_block_native_vs_casm(
    block_number: u64,
    config: &RpcStateReaderConfig,
    chain_info: &ChainInfo,
    native_manager: &ContractClassManager,
    casm_manager: &ContractClassManager,
    prefetch_initial_reads: bool,
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
        prefetch_initial_reads,
    );
    native_readers.min_sierra_version_override = min_sierra_version_override.clone();
    let ReexecuteBlockOutcome { actual_state_diff: native_state_diff, .. } =
        native_readers.reexecute_block()?;

    let mut casm_readers = ConsecutiveRpcStateReaders::new(
        prev_block,
        Some(config.clone()),
        chain_info.clone(),
        false,
        casm_manager.clone(),
        prefetch_initial_reads,
    );
    casm_readers.min_sierra_version_override = min_sierra_version_override;
    let ReexecuteBlockOutcome { actual_state_diff: casm_state_diff, .. } =
        casm_readers.reexecute_block()?;

    Ok(compare_state_diffs(native_state_diff, casm_state_diff, BlockNumber(block_number)))
}

fn is_block_not_found(err: &ReexecutionError) -> bool {
    matches!(err, ReexecutionError::Rpc(RPCStateReaderError::BlockNotFound(_)))
}
