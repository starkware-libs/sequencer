use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
) {
    let rpc_state_reader_config = RpcStateReaderConfig::from_url(node_url);
    let chain_info = get_chain_info(&chain_id, None);
    let block_counter = Arc::new(AtomicU64::new(start_block));

    println!(
        "Starting RPC replay from block {start_block}{} with {parallelism} workers.",
        end_block.map_or(" (indefinitely)".to_string(), |end| format!(" to {end}"))
    );

    let mut task_set = tokio::task::JoinSet::new();
    for _ in 0..parallelism {
        let counter = block_counter.clone();
        let config = rpc_state_reader_config.clone();
        let chain_info = chain_info.clone();
        let contract_class_manager = contract_class_manager.clone();

        task_set.spawn_blocking(move || {
            replay_worker(counter, end_block, &config, &chain_info, &contract_class_manager);
        });
    }
    task_set.join_all().await;
}

fn replay_worker(
    block_counter: Arc<AtomicU64>,
    end_block: Option<u64>,
    config: &RpcStateReaderConfig,
    chain_info: &ChainInfo,
    contract_class_manager: &ContractClassManager,
) {
    loop {
        // Relaxed ordering is fine: each block is independent and we only need a unique
        // assignment. With multiple workers, up to `parallelism - 1` threads may grab a
        // number past `end_block` before checking and exiting -- this is harmless.
        let block_number = block_counter.fetch_add(1, Ordering::Relaxed);
        if let Some(end) = end_block {
            if block_number > end {
                break;
            }
        }

        // Wait-for-tip retry loop.
        let result = loop {
            match reexecute_block(block_number, config, chain_info, contract_class_manager) {
                Err(ref e) if is_block_not_found(e) => {
                    println!("Block {block_number} not found, waiting for chain tip...");
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                result => break result,
            }
        };

        match result {
            Ok(true) => println!("Block {block_number} passed."),
            Ok(false) => {} // Mismatch already logged inside.
            Err(e) => eprintln!("ERROR block {block_number}: {e}"),
        }
    }
}

/// Reexecutes a single block via RPC and compares state diffs.
/// Returns `Ok(true)` if diffs match, `Ok(false)` if mismatch (logged to stderr).
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

    let (_block_state, expected_state_diff, actual_state_diff) = readers.reexecute_block()?;

    Ok(compare_state_diffs(expected_state_diff, actual_state_diff, Some(block_number)))
}

fn is_block_not_found(err: &ReexecutionError) -> bool {
    matches!(err, ReexecutionError::Rpc(RPCStateReaderError::BlockNotFound(_)))
}
