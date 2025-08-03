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
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff, StateMaps};
use blockifier::test_utils::maybe_dummy_block_hash_and_number;
use blockifier::transaction::transaction_execution::Transaction;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::types::layout_name::LayoutName;
use starknet_api::block::BlockHash;
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash as StarknetAPICompiledClassHash,
    ContractAddress,
};
use starknet_api::declare_tx_args;
use starknet_api::executable_transaction::{AccountTransaction, DeclareTransaction};
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::CHAIN_ID_FOR_TESTS;
use starknet_api::transaction::fields::ValidResourceBounds;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{
    try_node_index_into_contract_address,
    try_node_index_into_patricia_key,
    ConfigImpl,
    Input,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_os::io::os_input::{
    CachedStateInput,
    CommitmentInfo,
    OsBlockInput,
    OsChainInfo,
    OsHints,
    OsHintsConfig,
    StarknetOsInput,
};
use starknet_os::io::os_output::StarknetOsRunnerOutput;
use starknet_os::runner::run_os_stateless;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use starknet_patricia::test_utils::filter_inner_nodes_from_commitments;
use starknet_patricia_storage::map_storage::BorrowedMapStorage;
use starknet_types_core::felt::Felt;

use crate::initial_state::{InitialStateData, OsExecutionContracts};
use crate::state_trait::FlowTestState;
use crate::test_manager::STRK_FEE_TOKEN_ADDRESS;

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

pub(crate) struct CommitmentInfos {
    pub(crate) contracts_trie_commitment_info: CommitmentInfo,
    pub(crate) classes_trie_commitment_info: CommitmentInfo,
    pub(crate) storage_tries_commitment_infos: HashMap<ContractAddress, CommitmentInfo>,
}

/// Creates the commitment infos and the cached state input for the OS.
pub(crate) fn create_cached_state_input_and_commitment_infos(
    previous_commitment: &CommitmentOutput,
    new_commitment: &CommitmentOutput,
    commitments: &mut BorrowedMapStorage<'_>,
    extended_state_diff: &StateMaps,
) -> (CachedStateInput, CommitmentInfos) {
    // TODO(Nimrod): Gather the keys from the state selector similarly to python.
    let (previous_contract_states, new_storage_roots) = get_previous_states_and_new_storage_roots(
        extended_state_diff.get_contract_addresses().into_iter(),
        previous_commitment.contracts_trie_root_hash,
        new_commitment.contracts_trie_root_hash,
        commitments,
    );
    let mut address_to_previous_class_hash = HashMap::new();
    let mut address_to_previous_nonce = HashMap::new();
    let mut address_to_previous_storage_root_hash = HashMap::new();
    for (address, contract_state) in previous_contract_states.into_iter() {
        let address = try_node_index_into_contract_address(&address).unwrap();
        address_to_previous_class_hash.insert(address, contract_state.class_hash);
        address_to_previous_nonce.insert(address, contract_state.nonce);
        address_to_previous_storage_root_hash.insert(address, contract_state.storage_root_hash);
    }

    // Get previous class leaves.
    let mut class_leaf_indices: Vec<NodeIndex> = extended_state_diff
        .compiled_class_hashes
        .keys()
        .chain(extended_state_diff.declared_contracts.keys())
        .map(|address| NodeIndex::from_leaf_felt(&address.0))
        .collect();

    let sorted_class_leaf_indices = SortedLeafIndices::new(&mut class_leaf_indices);
    let previous_class_leaves: HashMap<NodeIndex, CompiledClassHash> =
        OriginalSkeletonTreeImpl::get_leaves(
            commitments,
            previous_commitment.classes_trie_root_hash,
            sorted_class_leaf_indices,
        )
        .unwrap();
    let class_hash_to_compiled_class_hash = previous_class_leaves
        .into_iter()
        .map(|(idx, v)| {
            (
                ClassHash(*try_node_index_into_patricia_key(&idx).unwrap().key()),
                StarknetAPICompiledClassHash(v.0),
            )
        })
        .collect();

    let mut storage = HashMap::new();
    for address in extended_state_diff.get_contract_addresses() {
        let mut storage_keys_indices: Vec<NodeIndex> = extended_state_diff
            .storage
            .keys()
            .filter_map(|(add, key)| {
                if add == &address { Some(NodeIndex::from_leaf_felt(&key.0)) } else { None }
            })
            .collect();
        let sorted_leaf_indices = SortedLeafIndices::new(&mut storage_keys_indices);
        let previous_storage_leaves: HashMap<NodeIndex, StarknetStorageValue> =
            OriginalSkeletonTreeImpl::get_leaves(
                commitments,
                address_to_previous_storage_root_hash[&address],
                sorted_leaf_indices,
            )
            .unwrap();
        let previous_storage_leaves: HashMap<StorageKey, Felt> = previous_storage_leaves
            .into_iter()
            .map(|(idx, v)| (StorageKey(try_node_index_into_patricia_key(&idx).unwrap()), v.0))
            .collect();
        storage.insert(address, previous_storage_leaves);
    }
    // Note: The generic type `<CompiledClassHash>` here is arbitrary.
    let commitments = filter_inner_nodes_from_commitments::<CompiledClassHash>(commitments.storage);
    let contracts_trie_commitment_info = CommitmentInfo {
        previous_root: previous_commitment.contracts_trie_root_hash,
        updated_root: new_commitment.contracts_trie_root_hash,
        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: commitments.clone(),
    };
    let classes_trie_commitment_info = CommitmentInfo {
        previous_root: previous_commitment.classes_trie_root_hash,
        updated_root: new_commitment.classes_trie_root_hash,
        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: commitments.clone(),
    };
    let storage_tries_commitment_infos = address_to_previous_storage_root_hash
        .iter()
        .map(|(address, previous_root_hash)| {
            (
                *address,
                CommitmentInfo {
                    previous_root: *previous_root_hash,
                    updated_root: new_storage_roots[address],
                    tree_height: SubTreeHeight::ACTUAL_HEIGHT,
                    commitment_facts: commitments.clone(),
                },
            )
        })
        .collect();
    (
        CachedStateInput {
            storage,
            address_to_class_hash: address_to_previous_class_hash,
            address_to_nonce: address_to_previous_nonce,
            class_hash_to_compiled_class_hash,
        },
        CommitmentInfos {
            contracts_trie_commitment_info,
            classes_trie_commitment_info,
            storage_tries_commitment_infos,
        },
    )
}

pub(crate) fn get_previous_states_and_new_storage_roots<I: Iterator<Item = ContractAddress>>(
    contract_addresses: I,
    previous_contract_trie_root: HashOutput,
    new_contract_trie_root: HashOutput,
    commitments: &BorrowedMapStorage<'_>,
) -> (HashMap<NodeIndex, ContractState>, HashMap<ContractAddress, HashOutput>) {
    let mut contract_leaf_indices: Vec<NodeIndex> =
        contract_addresses.map(|address| NodeIndex::from_leaf_felt(&address.0)).collect();

    // Get previous contract state leaves.
    let sorted_contract_leaf_indices = SortedLeafIndices::new(&mut contract_leaf_indices);
    // Get the previous and the new contract states.
    let previous_contract_states = OriginalSkeletonTreeImpl::get_leaves(
        commitments,
        previous_contract_trie_root,
        sorted_contract_leaf_indices,
    )
    .unwrap();
    let new_contract_states: HashMap<NodeIndex, ContractState> =
        OriginalSkeletonTreeImpl::get_leaves(
            commitments,
            new_contract_trie_root,
            sorted_contract_leaf_indices,
        )
        .unwrap();
    let new_contract_roots: HashMap<ContractAddress, HashOutput> = new_contract_states
        .into_iter()
        .map(|(idx, contract_state)| {
            (try_node_index_into_contract_address(&idx).unwrap(), contract_state.storage_root_hash)
        })
        .collect();
    (previous_contract_states, new_contract_roots)
}

pub(crate) async fn flow_test_body<S: FlowTestState>(
    mut initial_state_data: InitialStateData<S>,
    per_block_txs: Vec<Vec<Transaction>>,
    block_contexts: Vec<BlockContext>,
) -> StarknetOsRunnerOutput {
    // Execute the transactions.
    let mut os_block_inputs = vec![];
    let mut cached_state_inputs = vec![];
    let mut state = initial_state_data.initial_state.updatable_state;
    assert_eq!(per_block_txs.len(), block_contexts.len());
    let mut contracts_trie_root_hash = initial_state_data.initial_state.contracts_trie_root_hash;
    let mut classes_trie_root_hash = initial_state_data.initial_state.classes_trie_root_hash;
    for (block_txs, block_context) in per_block_txs.into_iter().zip(block_contexts.into_iter()) {
        let block_info = block_context.block_info().clone();
        let ExecutionOutput { execution_outputs, block_summary, mut final_state } =
            execute_transactions(state, &block_txs, block_context);
        let extended_state_diff = final_state.cache.borrow().extended_state_diff();
        // Update the wrapped state.
        let state_diff = final_state.to_state_diff().unwrap();
        state = final_state.state;
        state.apply_writes(&state_diff.state_maps, &final_state.class_hash_to_class.borrow());
        // Commit the state diff.
        let committer_state_diff = create_committer_state_diff(block_summary.state_diff);
        let mut map_storage = BorrowedMapStorage {
            storage: &mut initial_state_data.initial_state.commitment_storage,
        };
        let CommitmentOutput {
            contracts_trie_root_hash: new_contracts_trie_root_hash,
            classes_trie_root_hash: new_classes_trie_root_hash,
        } = commit_state_diff(
            &mut map_storage,
            contracts_trie_root_hash,
            classes_trie_root_hash,
            committer_state_diff,
        )
        .await;

        let previous_commitment =
            CommitmentOutput { contracts_trie_root_hash, classes_trie_root_hash };
        let new_commitment = CommitmentOutput {
            contracts_trie_root_hash: new_contracts_trie_root_hash,
            classes_trie_root_hash: new_classes_trie_root_hash,
        };
        // Prepare the OS input.
        let (cached_state_input, commitment_infos) = create_cached_state_input_and_commitment_infos(
            &previous_commitment,
            &new_commitment,
            &mut map_storage,
            &extended_state_diff,
        );
        let tx_execution_infos = execution_outputs
            .into_iter()
            .map(|(execution_info, _)| execution_info.into())
            .collect();
        let block_txs = block_txs.into_iter().map(Into::into).collect();
        let old_block_number_and_hash =
            maybe_dummy_block_hash_and_number(block_info.block_number).map(|v| (v.number, v.hash));

        // TODO(Nimrod): Verify this.
        let class_hashes_to_migrate = HashMap::new();
        let os_block_input = OsBlockInput {
            contract_state_commitment_info: commitment_infos.contracts_trie_commitment_info,
            contract_class_commitment_info: commitment_infos.classes_trie_commitment_info,
            address_to_storage_commitment_info: commitment_infos.storage_tries_commitment_infos,
            transactions: block_txs,
            tx_execution_infos,
            declared_class_hash_to_component_hashes: initial_state_data
                .execution_contracts
                .declared_class_hash_to_component_hashes
                .clone(),
            block_info,
            // Dummy block hashes.
            prev_block_hash: BlockHash(Felt::ONE),
            new_block_hash: BlockHash(Felt::ONE),
            old_block_number_and_hash,
            class_hashes_to_migrate,
        };
        os_block_inputs.push(os_block_input);
        cached_state_inputs.push(cached_state_input);
        contracts_trie_root_hash = new_contracts_trie_root_hash;
        classes_trie_root_hash = new_classes_trie_root_hash;
    }
    let starknet_os_input = StarknetOsInput {
        os_block_inputs,
        cached_state_inputs,
        deprecated_compiled_classes: initial_state_data
            .execution_contracts
            .executed_contracts
            .deprecated_contracts
            .into_iter()
            .collect(),
        compiled_classes: initial_state_data
            .execution_contracts
            .executed_contracts
            .contracts
            .into_iter()
            .collect(),
    };
    let chain_info = OsChainInfo {
        chain_id: CHAIN_ID_FOR_TESTS.clone(),
        strk_fee_token_address: *STRK_FEE_TOKEN_ADDRESS,
    };
    let os_hints_config = OsHintsConfig { chain_info, ..Default::default() };
    let os_hints = OsHints { os_input: starknet_os_input, os_hints_config };
    let layout = LayoutName::all_cairo;

    run_os_stateless(layout, os_hints).unwrap()
}
