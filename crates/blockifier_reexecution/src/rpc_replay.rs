use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use blockifier::blockifier::config::{CairoNativeMode, ContractClassManagerConfig};
use blockifier::context::ChainInfo;
use blockifier::state::contract_class_manager::ContractClassManager;
use starknet_api::block::BlockNumber;

use crate::errors::{RPCStateReaderError, ReexecutionError, ReexecutionResult};
use crate::state_reader::config::RpcStateReaderConfig;
use crate::state_reader::reexecution_state_reader::ConsecutiveReexecutionStateReaders;
use crate::state_reader::rpc_state_reader::ConsecutiveRpcStateReaders;
use crate::utils::{compare_state_diffs, get_chain_info};

/// Runs continuous RPC replay from `start_block` to `end_block` (or forever if `None`),
/// using `parallelism` worker threads. Each block is reexecuted and the resulting state diff
/// is compared against the expected one from the chain.
pub async fn run_rpc_replay(
    node_url: String,
    chain_id: starknet_api::core::ChainId,
    start_block: u64,
    end_block: Option<u64>,
    parallelism: usize,
    contract_class_manager: ContractClassManager,
    compare_native: bool,
) {
    let rpc_state_reader_config = RpcStateReaderConfig::from_url(node_url);
    let chain_info = get_chain_info(&chain_id, None);
    let block_counter = Arc::new(AtomicU64::new(start_block));

    let mode_description = if compare_native { " (compare native vs CASM)" } else { "" };
    println!(
        "Starting RPC replay from block {start_block}{}{mode_description} with {parallelism} \
         workers.",
        end_block.map_or(" (indefinitely)".to_string(), |end| format!(" to {end}"))
    );

    let replay_mode = if compare_native {
        let native_manager = create_contract_class_manager(CairoNativeMode::WaitOnCompilation);
        let casm_manager = create_contract_class_manager(CairoNativeMode::Off);
        ReplayMode::CompareNative { native_manager, casm_manager }
    } else {
        ReplayMode::Standard { contract_class_manager }
    };

    let mut task_set = tokio::task::JoinSet::new();
    for _ in 0..parallelism {
        let counter = block_counter.clone();
        let config = rpc_state_reader_config.clone();
        let chain_info = chain_info.clone();
        let replay_mode = replay_mode.clone();

        task_set.spawn_blocking(move || {
            replay_worker(counter, end_block, &config, &chain_info, &replay_mode);
        });
    }
    task_set.join_all().await;
}

#[derive(Clone)]
enum ReplayMode {
    Standard { contract_class_manager: ContractClassManager },
    CompareNative { native_manager: ContractClassManager, casm_manager: ContractClassManager },
}

fn replay_worker(
    block_counter: Arc<AtomicU64>,
    end_block: Option<u64>,
    config: &RpcStateReaderConfig,
    chain_info: &ChainInfo,
    replay_mode: &ReplayMode,
) {
    loop {
        let block_number = block_counter.fetch_add(1, Ordering::Relaxed);
        if let Some(end) = end_block {
            if block_number > end {
                break;
            }
        }

        let result = retry_on_block_not_found(block_number, || match replay_mode {
            ReplayMode::Standard { contract_class_manager } => {
                reexecute_and_compare_to_chain(
                    block_number,
                    config,
                    chain_info,
                    contract_class_manager,
                )
            }
            ReplayMode::CompareNative { native_manager, casm_manager } => {
                compare_native_vs_casm(
                    block_number,
                    config,
                    chain_info,
                    native_manager,
                    casm_manager,
                )
            }
        });

        match result {
            Ok(true) => println!("Block {block_number} passed."),
            Ok(false) => {} // Mismatch already logged inside.
            Err(e) => eprintln!("ERROR block {block_number}: {e}"),
        }
    }
}

fn retry_on_block_not_found(
    block_number: u64,
    mut execute_block: impl FnMut() -> ReexecutionResult<bool>,
) -> ReexecutionResult<bool> {
    loop {
        match execute_block() {
            Err(ref e) if is_block_not_found(e) => {
                println!("Block {block_number} not found, waiting for chain tip...");
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            result => return result,
        }
    }
}

/// Reexecutes a single block via RPC and compares the actual state diff against the chain.
fn reexecute_and_compare_to_chain(
    block_number: u64,
    config: &RpcStateReaderConfig,
    chain_info: &ChainInfo,
    contract_class_manager: &ContractClassManager,
) -> ReexecutionResult<bool> {
    let readers = ConsecutiveRpcStateReaders::new(
        BlockNumber(block_number - 1),
        Some(config.clone()),
        chain_info.clone(),
        false,
        contract_class_manager.clone(),
    );

    let (_block_state, expected_state_diff, actual_state_diff) = readers.reexecute_block()?;

    Ok(compare_state_diffs(expected_state_diff, actual_state_diff, Some(block_number), None))
}

fn create_contract_class_manager(cairo_native_mode: CairoNativeMode) -> ContractClassManager {
    let mut config = ContractClassManagerConfig::default();
    config.cairo_native_run_config.cairo_native_mode = cairo_native_mode;
    ContractClassManager::start(config)
}

/// Reexecutes a single block twice — once with native and once with CASM — and compares the
/// resulting state diffs against each other.
#[cfg(feature = "cairo_native")]
fn compare_native_vs_casm(
    block_number: u64,
    config: &RpcStateReaderConfig,
    chain_info: &ChainInfo,
    native_manager: &ContractClassManager,
    casm_manager: &ContractClassManager,
) -> ReexecutionResult<bool> {
    let native_readers = ConsecutiveRpcStateReaders::new(
        BlockNumber(block_number - 1),
        Some(config.clone()),
        chain_info.clone(),
        false,
        native_manager.clone(),
    );
    let (_block_state, _expected, native_state_diff) = native_readers.reexecute_block()?;

    let casm_readers = ConsecutiveRpcStateReaders::new(
        BlockNumber(block_number - 1),
        Some(config.clone()),
        chain_info.clone(),
        false,
        casm_manager.clone(),
    );
    let (_block_state, _expected, casm_state_diff) = casm_readers.reexecute_block()?;

    Ok(compare_state_diffs(
        native_state_diff,
        casm_state_diff,
        Some(block_number),
        Some("native vs CASM"),
    ))
}

#[cfg(not(feature = "cairo_native"))]
fn compare_native_vs_casm(
    _block_number: u64,
    _config: &RpcStateReaderConfig,
    _chain_info: &ChainInfo,
    _native_manager: &ContractClassManager,
    _casm_manager: &ContractClassManager,
) -> ReexecutionResult<bool> {
    panic!("--compare-native requires the cairo_native feature");
}

fn is_block_not_found(err: &ReexecutionError) -> bool {
    matches!(err, ReexecutionError::Rpc(RPCStateReaderError::BlockNotFound(_)))
}
