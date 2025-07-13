use std::collections::HashMap;

use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutor,
    TransactionExecutorError,
};
use blockifier::context::BlockContext;
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff, StateMaps};
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::maybe_dummy_block_hash_and_number;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction;
use starknet_committer::block_committer::input::{
    ConfigImpl,
    Input,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};

#[allow(dead_code)]
pub(crate) type CommitterInput = Input<ConfigImpl>;

#[allow(dead_code)]
pub(crate) type ExecutionOutput = (
    Vec<(TransactionExecutionInfo, StateMaps)>,
    BlockExecutionSummary,
    CachedState<DictStateReader>,
);

#[allow(dead_code)]
pub(crate) fn create_committer_input(
    state_diff: CommitmentStateDiff,
    fact_storage: HashMap<DbKey, DbValue>,
    contracts_trie_root_hash: HashOutput,
    classes_trie_root_hash: HashOutput,
) -> CommitterInput {
    let state_diff = StateDiff {
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
    };
    let config = ConfigImpl::default();

    CommitterInput {
        state_diff,
        storage: fact_storage,
        contracts_trie_root_hash,
        classes_trie_root_hash,
        config,
    }
}

#[allow(dead_code)]
pub(crate) fn execute_transactions(
    initial_state: Option<DictStateReader>,
    txs: &[Transaction],
) -> ExecutionOutput {
    let initial_state_reader = initial_state.unwrap_or_default();
    let dummy_block_context = BlockContext::create_for_testing();
    let block_number_hash_pair =
        maybe_dummy_block_hash_and_number(dummy_block_context.block_info().block_number);
    let config = TransactionExecutorConfig::default();
    let mut executor = TransactionExecutor::pre_process_and_create(
        initial_state_reader,
        dummy_block_context,
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
