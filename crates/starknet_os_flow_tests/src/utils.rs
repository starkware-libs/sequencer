#![allow(dead_code)]
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutionOutput,
    TransactionExecutor,
    TransactionExecutorError,
};
use blockifier::context::BlockContext;
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff};
use blockifier::test_utils::maybe_dummy_block_hash_and_number;
use blockifier::transaction::transaction_execution::Transaction;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{
    ConfigImpl,
    Input,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::map_storage::MapStorage;

use crate::state_trait::FlowTestState;

pub(crate) type ExecutionOutput<S> =
    (Vec<TransactionExecutionOutput>, BlockExecutionSummary, CachedState<S>);

/// Executes the given transactions on the given state and block context with default execution
/// configuration.
pub(crate) fn execute_transactions<S: FlowTestState>(
    initial_state_reader: S,
    txs: &[Transaction],
    block_context: BlockContext,
) -> ExecutionOutput<S> {
    let block_number_hash_pair =
        maybe_dummy_block_hash_and_number(block_context.block_info().block_number);
    let config = TransactionExecutorConfig::default();
    let mut executor = TransactionExecutor::pre_process_and_create(
        initial_state_reader,
        block_context,
        block_number_hash_pair,
        config,
    )
    .expect("Failed to create transaction executor.");

    // Execute the transactions and make sure none of them failed.
    let execution_deadline = None;
    let execution_outputs = executor
        .execute_txs(txs, execution_deadline)
        .into_iter()
        .collect::<Result<_, TransactionExecutorError>>()
        .expect("Unexpected error during execution.");

    // Finalize the block to get the state diff.
    let block_summary = executor.finalize().expect("Failed to finalize block.");
    let final_state = executor.block_state.unwrap();
    (execution_outputs, block_summary, final_state)
}

/// Creates a state diff input for the committer based on the execution state diff.
pub(crate) fn create_committer_state_diff(state_diff: CommitmentStateDiff) -> StateDiff {
    StateDiff {
        address_to_class_hash: state_diff.address_to_class_hash.into_iter().collect(),
        address_to_nonce: state_diff.address_to_nonce.into_iter().collect(),
        class_hash_to_compiled_class_hash: state_diff
            .class_hash_to_compiled_class_hash
            .into_iter()
            .map(|(k, v)| (k, CompiledClassHash(v.0)))
            .collect(),
        storage_updates: state_diff
            .storage_updates
            .into_iter()
            .map(|(address, updates)| {
                (
                    address,
                    updates
                        .into_iter()
                        .map(|(k, v)| (StarknetStorageKey(k), StarknetStorageValue(v)))
                        .collect(),
                )
            })
            .collect(),
    }
}

/// Commits the state diff, saves the new facts and returns the computed roots.
async fn commit_state_diff(
    facts: &mut MapStorage,
    contracts_trie_root_hash: HashOutput,
    classes_trie_root_hash: HashOutput,
    state_diff: StateDiff,
) -> (HashOutput, HashOutput) {
    let config = ConfigImpl::default();
    // TODO(Nimrod): Remove the clone once commit takes reference to the storage.
    let input = Input {
        storage: facts.storage.clone(),
        state_diff,
        contracts_trie_root_hash,
        classes_trie_root_hash,
        config,
    };
    let filled_forest = commit_block(input).await.expect("Failed to commit the given block.");
    filled_forest.write_to_storage(facts);
    (filled_forest.get_contract_root_hash(), filled_forest.get_compiled_class_root_hash())
}
