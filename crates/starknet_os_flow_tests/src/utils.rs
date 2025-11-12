#![allow(dead_code)]

use std::collections::HashMap;

use assert_matches::assert_matches;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutionOutput,
    TransactionExecutor,
};
use blockifier::context::BlockContext;
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff, StateMaps};
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::transaction::transaction_execution::Transaction;
use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash as StarknetAPICompiledClassHash,
    ContractAddress,
    Nonce,
};
use starknet_api::declare_tx_args;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::executable_transaction::{AccountTransaction, DeclareTransaction};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_api::state::StorageKey;
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::{NonceManager, CHAIN_ID_FOR_TESTS};
use starknet_api::transaction::fields::{Fee, ValidResourceBounds};
use starknet_api::transaction::TransactionVersion;
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
use starknet_committer::patricia_merkle_tree::tree::fetch_previous_and_new_patricia_paths;
use starknet_committer::patricia_merkle_tree::types::{
    CompiledClassHash,
    RootHashes,
    StarknetForestProofs,
};
use starknet_os::hints::hint_implementation::deprecated_compiled_class::class_hash::compute_deprecated_class_hash;
use starknet_os::io::os_input::{CachedStateInput, CommitmentInfo};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::flatten_preimages;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;

use crate::initial_state::OsExecutionContracts;
use crate::state_trait::FlowTestState;
use crate::test_manager::FUNDED_ACCOUNT_ADDRESS;
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;

pub(crate) struct ExecutionOutput<S: FlowTestState> {
    pub(crate) execution_outputs: Vec<TransactionExecutionOutput>,
    pub(crate) block_summary: BlockExecutionSummary,
    pub(crate) final_state: CachedState<S>,
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

    // Execute.
    let mut executor = TransactionExecutor::pre_process_and_create(
        initial_state,
        block_context,
        block_number_hash_pair.map(|(number, hash)| BlockHashAndNumber { hash, number }),
        config,
    )
    .expect("Failed to create transaction executor.");

    // Execute the transactions and make sure none of them failed.
    let execution_deadline = None;
    let execution_results =
        executor.execute_txs(txs, execution_deadline).into_iter().collect::<Vec<Result<_, _>>>();
    let mut execution_outputs = Vec::new();
    for (tx_index, result) in execution_results.into_iter().enumerate() {
        match result {
            Ok(output) => execution_outputs.push(output),
            Err(error) => {
                panic!("Unexpected error during execution of tx at index {tx_index}: {error:?}.");
            }
        }
    }

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
    commitments: &mut MapStorage,
    contracts_trie_root_hash: HashOutput,
    classes_trie_root_hash: HashOutput,
    state_diff: StateDiff,
) -> StateRoots {
    let config = ConfigImpl::default();
    let input = Input { state_diff, contracts_trie_root_hash, classes_trie_root_hash, config };
    let filled_forest =
        commit_block(input, commitments, None).await.expect("Failed to commit the given block.");
    filled_forest.write_to_storage(commitments);
    StateRoots {
        contracts_trie_root_hash: filled_forest.get_contract_root_hash(),
        classes_trie_root_hash: filled_forest.get_compiled_class_root_hash(),
    }
}

pub(crate) fn create_cairo1_bootstrap_declare_tx(
    feature_contract: FeatureContract,
    execution_contracts: &mut OsExecutionContracts,
) -> AccountTransaction {
    assert_matches!(feature_contract.cairo_version(), CairoVersion::Cairo1(_));
    create_declare_tx(feature_contract, &mut NonceManager::default(), execution_contracts, true)
}

/// Create a declare tx from the funded account. Optionally creates the tx as a bootstrap-declare.
pub(crate) fn create_declare_tx(
    feature_contract: FeatureContract,
    nonce_manager: &mut NonceManager,
    execution_contracts: &mut OsExecutionContracts,
    bootstrap: bool,
) -> AccountTransaction {
    let sender_address =
        if bootstrap { DeclareTransaction::bootstrap_address() } else { *FUNDED_ACCOUNT_ADDRESS };
    let nonce = if bootstrap { Nonce::default() } else { nonce_manager.next(sender_address) };
    let declare_args = match feature_contract.get_class() {
        ContractClass::V0(class) => {
            let class_hash = ClassHash(compute_deprecated_class_hash(&class).unwrap());
            execution_contracts.add_deprecated_contract(class_hash, class);
            declare_tx_args! {
                version: TransactionVersion::ONE,
                max_fee: if bootstrap { Fee::default() } else { Fee(1_000_000_000_000_000) },
                class_hash,
                sender_address,
                nonce,
            }
        }
        ContractClass::V1((casm, _sierra_version)) => {
            let sierra = feature_contract.get_sierra();
            let class_hash = sierra.calculate_class_hash();
            let compiled_class_hash = feature_contract.get_compiled_class_hash(&HashVersion::V2);
            execution_contracts.add_cairo1_contract(casm, &sierra);
            declare_tx_args! {
                sender_address,
                class_hash,
                compiled_class_hash,
                resource_bounds: if bootstrap {
                    ValidResourceBounds::create_for_testing_no_fee_enforcement()
                } else {
                    *NON_TRIVIAL_RESOURCE_BOUNDS
                },
                nonce,
            }
        }
    };
    let account_declare_tx = declare_tx(declare_args);
    let class_info = get_class_info_of_feature_contract(feature_contract);
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
    previous_state_roots: &StateRoots,
    new_state_roots: &StateRoots,
    commitments: &mut MapStorage,
    extended_state_diff: &StateMaps,
) -> (CachedStateInput, CommitmentInfos) {
    // TODO(Nimrod): Gather the keys from the state selector similarly to python.
    let (previous_contract_states, new_storage_roots) = get_previous_states_and_new_storage_roots(
        extended_state_diff.get_contract_addresses().into_iter(),
        previous_state_roots.contracts_trie_root_hash,
        new_state_roots.contracts_trie_root_hash,
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
            previous_state_roots.classes_trie_root_hash,
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

    let storage_proofs = fetch_storage_proofs_from_state_maps(
        extended_state_diff,
        commitments,
        RootHashes {
            previous_root_hash: previous_state_roots.classes_trie_root_hash,
            new_root_hash: new_state_roots.classes_trie_root_hash,
        },
        RootHashes {
            previous_root_hash: previous_state_roots.contracts_trie_root_hash,
            new_root_hash: new_state_roots.contracts_trie_root_hash,
        },
    );
    let contracts_trie_commitment_info = CommitmentInfo {
        previous_root: previous_state_roots.contracts_trie_root_hash,
        updated_root: new_state_roots.contracts_trie_root_hash,
        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: flatten_preimages(&storage_proofs.contracts_trie_proof.nodes),
    };
    let classes_trie_commitment_info = CommitmentInfo {
        previous_root: previous_state_roots.classes_trie_root_hash,
        updated_root: new_state_roots.classes_trie_root_hash,
        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: flatten_preimages(&storage_proofs.classes_trie_proof),
    };
    let storage_tries_commitment_infos = address_to_previous_storage_root_hash
        .iter()
        .map(|(address, previous_root_hash)| {
            // Not all contracts in `address_to_previous_storage_root_hash` are in
            // `extended_state_diff`. For example a contract that only its Nonce was
            // changed.
            let storage_proof = flatten_preimages(
                storage_proofs
                    .contracts_trie_storage_proofs
                    .get(address)
                    .unwrap_or(&HashMap::new()),
            );
            (
                *address,
                CommitmentInfo {
                    previous_root: *previous_root_hash,
                    updated_root: new_storage_roots[address],
                    tree_height: SubTreeHeight::ACTUAL_HEIGHT,
                    commitment_facts: storage_proof,
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
    commitments: &mut MapStorage,
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

pub(crate) fn maybe_dummy_block_hash_and_number(
    block_number: BlockNumber,
) -> Option<(BlockNumber, BlockHash)> {
    if block_number.0 < STORED_BLOCK_HASH_BUFFER {
        return None;
    }
    let block_hash = BlockHash(Felt::from(block_number.0));
    let block_number = BlockNumber(block_number.0 - STORED_BLOCK_HASH_BUFFER);
    Some((block_number, block_hash))
}

pub(crate) fn divide_vec_into_n_parts<T>(mut vec: Vec<T>, n: usize) -> Vec<Vec<T>> {
    assert!(n > 0, "Number of parts must be positive");
    let minimal_items_per_part = vec.len() / n;
    let remainder = vec.len() % n;
    let mut items_per_part = Vec::with_capacity(n);
    for i in 0..n {
        let part_len = minimal_items_per_part + usize::from(i < remainder);
        let part: Vec<T> = vec.drain(0..part_len).collect();
        items_per_part.push(part);
    }
    assert_eq!(n, items_per_part.len(), "Number of parts does not match.");
    items_per_part
}

pub(crate) fn get_class_info_of_cairo0_contract(
    contract_class: DeprecatedContractClass,
) -> ClassInfo {
    let abi_length = contract_class.abi.as_ref().unwrap().len();
    ClassInfo {
        contract_class: ContractClass::V0(contract_class),
        sierra_program_length: 0,
        abi_length,
        sierra_version: SierraVersion::DEPRECATED,
    }
}

pub(crate) fn get_class_info_of_feature_contract(feature_contract: FeatureContract) -> ClassInfo {
    match feature_contract.get_class() {
        ContractClass::V0(contract_class) => get_class_info_of_cairo0_contract(contract_class),
        ContractClass::V1((contract_class, sierra_version)) => {
            let sierra = feature_contract.get_sierra();
            ClassInfo {
                contract_class: ContractClass::V1((contract_class, sierra_version.clone())),
                sierra_program_length: sierra.sierra_program.len(),
                abi_length: sierra.abi.len(),
                sierra_version,
            }
        }
    }
}

pub(crate) fn get_class_hash_of_feature_contract(feature_contract: FeatureContract) -> ClassHash {
    match feature_contract.get_class() {
        ContractClass::V0(class) => ClassHash(compute_deprecated_class_hash(&class).unwrap()),
        ContractClass::V1(_) => feature_contract.get_sierra().calculate_class_hash(),
    }
}

fn fetch_storage_proofs_from_state_maps(
    state_maps: &StateMaps,
    storage: &mut MapStorage,
    classes_trie_root_hashes: RootHashes,
    contracts_trie_root_hashes: RootHashes,
) -> StarknetForestProofs {
    let class_hashes: Vec<ClassHash> = state_maps
        .compiled_class_hashes
        .keys()
        .cloned()
        .chain(state_maps.class_hashes.values().cloned())
        .collect();
    let contract_addresses =
        &state_maps.get_contract_addresses().iter().cloned().collect::<Vec<_>>();
    let contract_storage_keys = state_maps.storage.keys().fold(
        HashMap::<ContractAddress, Vec<StarknetStorageKey>>::new(),
        |mut acc, (address, key)| {
            acc.entry(*address).or_default().push(StarknetStorageKey(*key));
            acc
        },
    );

    fetch_previous_and_new_patricia_paths(
        storage,
        classes_trie_root_hashes,
        contracts_trie_root_hashes,
        &class_hashes,
        contract_addresses,
        &contract_storage_keys,
    )
    .unwrap()
}

/// Utility method to update a map of expected storage updates.
pub(crate) fn update_expected_storage(
    expected_storage_updates: &mut HashMap<
        ContractAddress,
        HashMap<StarknetStorageKey, StarknetStorageValue>,
    >,
    address: ContractAddress,
    key: Felt,
    value: Felt,
) {
    let key = StarknetStorageKey(StorageKey(key.try_into().unwrap()));
    let value = StarknetStorageValue(value);
    expected_storage_updates
        .entry(address)
        .and_modify(|map| {
            map.insert(key, value);
        })
        .or_insert_with(|| HashMap::from([(key, value)]));
}
