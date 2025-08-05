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
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::declare_tx_args;
use starknet_api::executable_transaction::{AccountTransaction, DeclareTransaction};
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
use starknet_patricia_storage::map_storage::BorrowedMapStorage;

use crate::initial_state::OsExecutionContracts;
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

pub(crate) fn create_cairo1_bootstrap_declare_tx(
    sierra: &SierraContractClass,
    casm: CasmContractClass,
    execution_contracts: &mut OsExecutionContracts,
) -> AccountTransaction {
    let class_hash = sierra.calculate_class_hash();
    let compiled_class_hash = starknet_api::core::CompiledClassHash(casm.compiled_class_hash());
    execution_contracts.add_cairo1_contract(casm.clone(), sierra);
    let declare_tx_args = declare_tx_args! {
        sender_address: DeclareTransaction::bootstrap_address(),
        class_hash,
        compiled_class_hash,
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
    };
    let account_declare_tx = declare_tx(declare_tx_args);
    let sierra_version = SierraVersion::extract_from_program(&sierra.sierra_program).unwrap();
    let contract_class = ContractClass::V1((casm, sierra_version.clone()));
    let class_info = ClassInfo {
        contract_class,
        sierra_program_length: sierra.sierra_program.len(),
        abi_length: sierra.abi.len(),
        sierra_version,
    };
    let tx =
        DeclareTransaction::create(account_declare_tx, class_info, &CHAIN_ID_FOR_TESTS).unwrap();
    AccountTransaction::Declare(tx)
}
