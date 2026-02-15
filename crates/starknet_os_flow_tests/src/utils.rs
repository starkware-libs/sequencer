use std::collections::HashMap;

use assert_matches::assert_matches;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutionOutput,
    TransactionExecutor,
};
use blockifier::context::BlockContext;
use blockifier::state::cached_state::{CachedState, StateChangesKeys, StateMaps};
use blockifier::state::state_api::StateReader;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::transaction::transaction_execution::Transaction;
use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::declare_tx_args;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::executable_transaction::{AccountTransaction, DeclareTransaction};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_api::state::StorageKey;
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::{test_block_hash, NonceManager, CHAIN_ID_FOR_TESTS};
use starknet_api::transaction::fields::{Fee, ValidResourceBounds};
use starknet_api::transaction::TransactionVersion;
use starknet_committer::block_committer::commit::{CommitBlockImpl, CommitBlockTrait};
use starknet_committer::block_committer::input::{
    try_node_index_into_contract_address,
    try_node_index_into_patricia_key,
    Input,
    ReaderConfig,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::db::facts_db::create_facts_tree::get_leaves;
use starknet_committer::db::facts_db::db::FactsDb;
use starknet_committer::db::facts_db::types::FactsDbInitialRead;
use starknet_committer::db::forest_trait::ForestWriter;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::tree::fetch_previous_and_new_patricia_paths;
use starknet_committer::patricia_merkle_tree::types::{RootHashes, StarknetForestProofs};
use starknet_os::hints::hint_implementation::deprecated_compiled_class::class_hash::compute_deprecated_class_hash;
use starknet_os::hints::vars::Const;
use starknet_os::io::os_input::{CommitmentInfo, StateCommitmentInfos};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::flatten_preimages;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use starknet_patricia_storage::db_object::EmptyKeyContext;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;

use crate::initial_state::OsExecutionContracts;
use crate::test_manager::FUNDED_ACCOUNT_ADDRESS;
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;

pub(crate) struct ExecutionOutput<S: StateReader + Send> {
    pub(crate) execution_outputs: Vec<TransactionExecutionOutput>,
    pub(crate) final_state: CachedState<S>,
}

/// Executes the given transactions on the given state and block context with default execution
/// configuration.
pub(crate) fn execute_transactions<S: StateReader + Send>(
    initial_state: S,
    txs: &[Transaction],
    block_context: BlockContext,
    virtual_os: bool,
) -> ExecutionOutput<S> {
    let block_number_hash_pair =
        maybe_dummy_block_hash_and_number(block_context.block_info().block_number);
    let config = TransactionExecutorConfig::default();

    // Execute.
    let mut executor = if virtual_os {
        // In virtual OS mode, the executor is created without preprocessing.
        TransactionExecutor::new(CachedState::new(initial_state), block_context, config)
    } else {
        TransactionExecutor::pre_process_and_create(
            initial_state,
            block_context,
            block_number_hash_pair.map(|(number, hash)| BlockHashAndNumber { hash, number }),
            config,
        )
        .expect("Failed to create transaction executor.")
    };

    // Execute the transactions and make sure none of them failed.
    let execution_deadline = None;
    let execution_results = executor
        .execute_txs_sequentially(txs, execution_deadline)
        .into_iter()
        .collect::<Vec<Result<_, _>>>();
    let mut execution_outputs = Vec::new();
    for (tx_index, result) in execution_results.into_iter().enumerate() {
        match result {
            Ok(output) => execution_outputs.push(output),
            Err(error) => {
                panic!("Unexpected error during execution of tx at index {tx_index}: {error}.");
            }
        }
    }

    if !virtual_os {
        // Finalize the block.
        executor.finalize().expect("Failed to finalize block.");
    }

    let final_state = executor.block_state.unwrap();
    ExecutionOutput { execution_outputs, final_state }
}

/// Commits the state diff, saves the new commitments and returns the computed roots.
pub(crate) async fn commit_state_diff(
    facts_db: &mut FactsDb<MapStorage>,
    contracts_trie_root_hash: HashOutput,
    classes_trie_root_hash: HashOutput,
    state_diff: StateDiff,
) -> StateRoots {
    let config = ReaderConfig::default();
    let initial_read_context =
        FactsDbInitialRead(StateRoots { contracts_trie_root_hash, classes_trie_root_hash });
    let input = Input { state_diff, initial_read_context, config };
    let filled_forest = CommitBlockImpl::commit_block(input, facts_db, None)
        .await
        .expect("Failed to commit the given block.");
    facts_db.write(&filled_forest).await.expect("Failed to write filled forest to storage");
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

/// Gets the extended initial reads for the OS run.
/// The extended initial reads are the initial reads with the class hash and nonce of each accessed
/// contract.
pub(crate) fn get_extended_initial_reads<S: StateReader>(state: &CachedState<S>) -> StateMaps {
    let raw_initial_reads = state.get_initial_reads().unwrap();
    // Populate the state initial reads with the class hash and nonce of each accessed contract.
    for contract_address in raw_initial_reads.get_contract_addresses() {
        state.get_class_hash_at(contract_address).unwrap();
        state.get_nonce_at(contract_address).unwrap();
    }

    for class_hash in raw_initial_reads.declared_contracts.keys() {
        state.get_compiled_class_hash(*class_hash).unwrap();
    }

    // Take the initial reads again to get the updated initial reads.
    let mut extended_initial_reads = state.get_initial_reads().unwrap();
    // This field is not used by the OS, so we clear it.
    extended_initial_reads.declared_contracts.clear();
    extended_initial_reads
}

/// Creates the commitment infos for the OS.
pub(crate) async fn create_commitment_infos(
    previous_state_roots: &StateRoots,
    new_state_roots: &StateRoots,
    commitments: &mut MapStorage,
    initial_reads_keys: &StateChangesKeys,
) -> StateCommitmentInfos {
    let (previous_contract_states, new_storage_roots) = get_previous_states_and_new_storage_roots(
        initial_reads_keys.modified_contracts.iter().copied(),
        previous_state_roots.contracts_trie_root_hash,
        new_state_roots.contracts_trie_root_hash,
        commitments,
    )
    .await;
    let mut address_to_previous_storage_root_hash = HashMap::new();
    for (address, contract_state) in previous_contract_states.into_iter() {
        let address = try_node_index_into_contract_address(&address).unwrap();
        address_to_previous_storage_root_hash.insert(address, contract_state.storage_root_hash);
    }

    let mut storage = HashMap::new();
    for address in &initial_reads_keys.modified_contracts {
        let mut storage_keys_indices: Vec<NodeIndex> =
            initial_reads_keys
                .storage_keys
                .iter()
                .filter_map(|(add, key)| {
                    if add == address { Some(NodeIndex::from_leaf_felt(&key.0)) } else { None }
                })
                .collect();
        let sorted_leaf_indices = SortedLeafIndices::new(&mut storage_keys_indices);
        let previous_storage_leaves: HashMap<NodeIndex, StarknetStorageValue> = get_leaves(
            commitments,
            address_to_previous_storage_root_hash[address],
            sorted_leaf_indices,
            address,
        )
        .await
        .unwrap();
        for (idx, v) in previous_storage_leaves {
            let key = StorageKey(try_node_index_into_patricia_key(&idx).unwrap());
            storage.insert((*address, key), v.0);
        }
    }

    let storage_proofs = fetch_storage_proofs_from_state_changes_keys(
        initial_reads_keys,
        commitments,
        RootHashes {
            previous_root_hash: previous_state_roots.classes_trie_root_hash,
            new_root_hash: new_state_roots.classes_trie_root_hash,
        },
        RootHashes {
            previous_root_hash: previous_state_roots.contracts_trie_root_hash,
            new_root_hash: new_state_roots.contracts_trie_root_hash,
        },
    )
    .await;
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

    StateCommitmentInfos {
        contracts_trie_commitment_info,
        classes_trie_commitment_info,
        storage_tries_commitment_infos,
    }
}

pub(crate) async fn get_previous_states_and_new_storage_roots<
    I: Iterator<Item = ContractAddress>,
>(
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
    let previous_contract_states = get_leaves(
        commitments,
        previous_contract_trie_root,
        sorted_contract_leaf_indices,
        &EmptyKeyContext,
    )
    .await
    .unwrap();
    let new_contract_states: HashMap<NodeIndex, ContractState> = get_leaves(
        commitments,
        new_contract_trie_root,
        sorted_contract_leaf_indices,
        &EmptyKeyContext,
    )
    .await
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
    let old_block_number = block_number.0 - STORED_BLOCK_HASH_BUFFER;
    let block_hash = test_block_hash(old_block_number);
    Some((BlockNumber(old_block_number), block_hash))
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

async fn fetch_storage_proofs_from_state_changes_keys(
    initial_reads_keys: &StateChangesKeys,
    storage: &mut MapStorage,
    classes_trie_root_hashes: RootHashes,
    contracts_trie_root_hashes: RootHashes,
) -> StarknetForestProofs {
    let class_hashes: Vec<ClassHash> =
        initial_reads_keys.compiled_class_hash_keys.iter().cloned().collect();
    let contract_addresses =
        &initial_reads_keys.modified_contracts.iter().cloned().collect::<Vec<_>>();
    let contract_storage_keys = initial_reads_keys.storage_keys.iter().fold(
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
    .await
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
    let key = StarknetStorageKey::try_from(key).unwrap();
    let value = StarknetStorageValue(value);
    expected_storage_updates
        .entry(address)
        .and_modify(|map| {
            map.insert(key, value);
        })
        .or_insert_with(|| HashMap::from([(key, value)]));
}

/// Given the first block number in the multiblock and the number of blocks in the multiblock,
/// update the expected storage updates for the block hash contract.
pub(crate) fn update_expected_storage_updates_for_block_hash_contract(
    expected_storage_updates: &mut HashMap<
        ContractAddress,
        HashMap<StarknetStorageKey, StarknetStorageValue>,
    >,
    first_block_number: BlockNumber,
    n_blocks_in_multi_block: usize,
) {
    // The OS is expected to write the (number -> hash) mapping of this block. Make sure the current
    // block number is greater than STORED_BLOCK_HASH_BUFFER.
    let old_block_number = first_block_number.0 - STORED_BLOCK_HASH_BUFFER;
    assert!(
        old_block_number > 0,
        "Block number must be big enough to test a non-trivial block hash mapping update."
    );

    // Add old block hashes to expected storage updates.
    let block_hash_contract_address = ContractAddress(
        PatriciaKey::try_from(Const::BlockHashContractAddress.fetch_from_os_program().unwrap())
            .unwrap(),
    );
    for block_number in first_block_number.0
        ..(first_block_number.0 + u64::try_from(n_blocks_in_multi_block).unwrap())
    {
        let (old_block_number, old_block_hash) =
            maybe_dummy_block_hash_and_number(BlockNumber(block_number)).unwrap();
        update_expected_storage(
            expected_storage_updates,
            block_hash_contract_address,
            Felt::from(old_block_number.0),
            old_block_hash.0,
        );
    }
}
