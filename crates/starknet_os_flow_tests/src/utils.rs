#![allow(dead_code)]
use std::collections::HashMap;

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
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::declare_tx_args;
use starknet_api::executable_transaction::DeclareTransaction;
use starknet_api::state::SierraContractClass;
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::CHAIN_ID_FOR_TESTS;
use starknet_api::transaction::fields::ValidResourceBounds;
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
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::PatriciaPrefix;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::NodeData;
use starknet_patricia_storage::map_storage::{BorrowedMapStorage, MapStorage};
use starknet_patricia_storage::storage_trait::DbKeyPrefix;
use starknet_types_core::felt::Felt;

use crate::state_trait::FlowTestState;

pub(crate) struct ExecutionOutput<S: FlowTestState> {
    pub(crate) execution_outputs: Vec<TransactionExecutionOutput>,
    pub(crate) block_summary: BlockExecutionSummary,
    pub(crate) final_state: CachedState<S>,
}

pub(crate) struct CommitmentOutput {
    pub(crate) contracts_trie_root_hash: HashOutput,
    pub(crate) classes_trie_root_hash: HashOutput,
}

/// Executes the given transactions on the given state and block context with default execution
/// configuration.
pub(crate) fn execute_transactions<S: FlowTestState>(
    initial_state: S,
    txs: &[Transaction],
    block_context: BlockContext,
) -> ExecutionOutput<S> {
    let block_number_hash_pair =
        maybe_dummy_block_hash_and_number(block_context.block_info().block_number);
    let config = TransactionExecutorConfig::default();
    let mut executor = TransactionExecutor::pre_process_and_create(
        initial_state,
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
    ExecutionOutput { execution_outputs, block_summary, final_state }
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

/// Commits the state diff, saves the new commitments and returns the computed roots.
pub(crate) async fn commit_state_diff(
    commitments: &mut BorrowedMapStorage<'_>,
    contracts_trie_root_hash: HashOutput,
    classes_trie_root_hash: HashOutput,
    state_diff: StateDiff,
) -> CommitmentOutput {
    let config = ConfigImpl::default();
    let input = Input { state_diff, contracts_trie_root_hash, classes_trie_root_hash, config };
    let filled_forest =
        commit_block(input, commitments.storage).await.expect("Failed to commit the given block.");
    filled_forest.write_to_storage(commitments);
    CommitmentOutput {
        contracts_trie_root_hash: filled_forest.get_contract_root_hash(),
        classes_trie_root_hash: filled_forest.get_compiled_class_root_hash(),
    }
}

pub(crate) fn create_cairo1_declare_tx(
    sierra: &SierraContractClass,
    casm: &CasmContractClass,
    sender_address: ContractAddress,
    nonce: Nonce,
    resource_bounds: ValidResourceBounds,
) -> starknet_api::executable_transaction::AccountTransaction {
    let class_hash = sierra.calculate_class_hash();
    let compiled_class_hash = starknet_api::core::CompiledClassHash(casm.compiled_class_hash());
    let declare_tx_args = declare_tx_args! {
        sender_address,
        class_hash,
        compiled_class_hash,
        resource_bounds,
        nonce,
    };
    let account_declare_tx = declare_tx(declare_tx_args);
    let sierra_version = SierraVersion::extract_from_program(&sierra.sierra_program).unwrap();
    let contract_class = ContractClass::V1((casm.clone(), sierra_version.clone()));
    let class_info = ClassInfo {
        contract_class,
        sierra_program_length: sierra.sierra_program.len(),
        abi_length: sierra.abi.len(),
        sierra_version,
    };
    let tx =
        DeclareTransaction::create(account_declare_tx, class_info, &CHAIN_ID_FOR_TESTS).unwrap();
    starknet_api::executable_transaction::AccountTransaction::Declare(tx)
}

// TODO(Nimrod): Remove once the committer has `fetch_witnesses` mechanism.
/// Filters inner nodes from the commitment storage for the commitment info that will be passed to
/// the OS.
/// Note: This produces many redundancy as the entire fact storage will be contained in each
/// commitment info.
pub(crate) fn filter_inner_nodes_from_commitments(
    commitments: &MapStorage,
) -> HashMap<HashOutput, Vec<Felt>> {
    let mut inner_nodes: HashMap<HashOutput, Vec<Felt>> = HashMap::new();
    let inner_node_prefix = DbKeyPrefix::from(PatriciaPrefix::InnerNode).to_bytes();
    for (key, value) in commitments.iter() {
        if let Some(suffix) = key.0.strip_prefix(inner_node_prefix) {
            // Note: The generic type `L`, `CompiledClassHash` is arbitrary here, as the result is
            // an inner node.
            let is_leaf = false;
            let hash = HashOutput(Felt::from_bytes_be_slice(&suffix[1..]));
            let node: FilledNode<CompiledClassHash> =
                FilledNode::deserialize(hash, value, is_leaf).unwrap();
            let flatten_value = match node.data {
                NodeData::Binary(data) => data.flatten(),
                NodeData::Edge(data) => data.flatten(),
                NodeData::Leaf(_) => panic!("Expected an inner node, but found a leaf."),
            };
            inner_nodes.insert(hash, flatten_value);
        }
    }
    inner_nodes
}
