#![allow(dead_code)]
use core::panic;
use std::collections::HashMap;

use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutionOutput,
    TransactionExecutor,
    TransactionExecutorError,
};
use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use blockifier::state::cached_state::{
    CachedState,
    CommitmentStateDiff,
    StateChangesKeys,
    StateMaps,
};
use blockifier::state::state_api::UpdatableState;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::maybe_dummy_block_hash_and_number;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::transaction_execution::Transaction;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::types::layout_name::LayoutName;
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber};
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash as StarknetAPICompiledClassHash,
    ContractAddress,
    Nonce,
};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableTransaction,
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction,
    Transaction as StarknetApiTransaction,
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::{ContractClassComponentHashes, SierraContractClass, StorageKey};
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::deploy_account::deploy_account_tx;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::test_utils::{
    CHAIN_ID_FOR_TESTS,
    CURRENT_BLOCK_NUMBER,
    DEFAULT_STRK_L1_GAS_PRICE,
};
use starknet_api::transaction::constants::DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_api::{declare_tx_args, deploy_account_tx_args, invoke_tx_args};
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{
    ConfigImpl,
    Input,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::tree::{
    OriginalSkeletonClassesTrieConfig,
    OriginalSkeletonContractsTrieConfig,
    OriginalSkeletonStorageTrieConfig,
};
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
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::PatriciaPrefix;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::NodeData;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTree,
    OriginalSkeletonTreeImpl,
};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{DbKey, DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

pub(crate) type CommitterInput = Input<ConfigImpl>;

pub(crate) type ExecutionOutput<S> =
    (Vec<TransactionExecutionOutput>, BlockExecutionSummary, CachedState<S>);

#[derive(Default)]
/// Gathers the information needed to execute a flow test.
pub(crate) struct InitialStateData<S: FlowTestState> {
    pub(crate) updatable_state: S,
    pub(crate) fact_storage: MapStorage,
    // Current patricia roots.
    pub(crate) contracts_trie_root_hash: HashOutput,
    pub(crate) classes_trie_root_hash: HashOutput,
    // Cairo contracts that run during the OS execution.
    pub(crate) contracts: HashMap<StarknetAPICompiledClassHash, CasmContractClass>,
    pub(crate) deprecated_contracts: HashMap<StarknetAPICompiledClassHash, DeprecatedContractClass>,
    // Sierra contracts that are declared during the OS execution.
    pub(crate) class_hash_to_sierra: HashMap<ClassHash, SierraContractClass>,
    pub(crate) fee_token_address: ContractAddress,
    // A funded account that is able to send some transactions and pay for them.
    pub(crate) funded_account: ContractAddress,
}

impl<S: FlowTestState> InitialStateData<S> {
    pub(crate) fn almost_dummy_block_context(&self) -> BlockContext {
        let mut dummy_block_context = BlockContext::create_for_testing();
        let mut dummy_chain_info = ChainInfo::create_for_testing();
        let mut dummy_block_info = BlockInfo::create_for_testing();
        dummy_block_info.block_number = BlockNumber(CURRENT_BLOCK_NUMBER + 1);
        dummy_chain_info.fee_token_addresses = FeeTokenAddresses {
            strk_fee_token_address: self.fee_token_address,
            eth_fee_token_address: ContractAddress::default(),
        };
        dummy_block_context.chain_info = dummy_chain_info;
        dummy_block_context.block_info = dummy_block_info;
        dummy_block_context
    }

    pub(crate) fn chain_info(&self) -> OsChainInfo {
        OsChainInfo {
            chain_id: CHAIN_ID_FOR_TESTS.clone(),
            strk_fee_token_address: self.fee_token_address,
        }
    }
}

/// Creates an input for the committer based on the execution state diff.
pub(crate) fn create_committer_input(
    state_diff: CommitmentStateDiff,
    fact_storage: &MapStorage,
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
        storage: fact_storage.storage.clone(),
        contracts_trie_root_hash,
        classes_trie_root_hash,
        config,
    }
}

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

pub(crate) async fn flow_test_body<S: FlowTestState>(
    initial_state: InitialStateData<S>,
    txs: Vec<Transaction>,
) -> StarknetOsRunnerOutput {
    // TODO(Nimrod): Validate the initial state.
    // Execute the transactions.
    let block_context = initial_state.almost_dummy_block_context();
    let chain_info = initial_state.chain_info();
    let block_info = block_context.block_info().clone();
    let mut os_block_inputs = vec![];
    let mut cached_state_inputs = vec![];
    let state = initial_state.updatable_state;

    let (txs_execution_outputs, execution_summary, state) =
        execute_transactions(state, &txs, block_context);
    // Prepare the committer input.
    let committer_input = create_committer_input(
        execution_summary.state_diff,
        &initial_state.fact_storage,
        initial_state.contracts_trie_root_hash,
        initial_state.classes_trie_root_hash,
    );
    let mut fact_storage = initial_state.fact_storage;

    // Run the committer and save the new facts.
    let filled_forest =
        commit_block(committer_input).await.expect("Failed to commit the given block.");
    filled_forest.write_to_storage(&mut fact_storage);
    let new_contracts_trie_root_hash = filled_forest.get_contract_root_hash();
    let new_classes_trie_root_hash = filled_forest.get_compiled_class_root_hash();
    let mut keys = state.cache.borrow().initial_reads.keys();
    keys.extend(&state.cache.borrow().writes.keys());
    // TODO(Nimrod): Remove this once the the keys are gathered from the state selector.
    let class_hash_extension: Vec<ClassHash> =
        initial_state.class_hash_to_sierra.keys().copied().collect();
    // Prepare the OS input.
    let (
        cached_state_input,
        contracts_trie_commitment_info,
        classes_trie_commitment_info,
        storage_tries_commitment_infos,
    ) = create_cached_state_input_and_commitment_infos(
        initial_state.contracts_trie_root_hash,
        new_contracts_trie_root_hash,
        initial_state.classes_trie_root_hash,
        new_classes_trie_root_hash,
        &fact_storage,
        keys,
        class_hash_extension,
    );
    let declared_contracts_component_hashes =
        get_declared_contracts_component_hashes(&txs, &initial_state.class_hash_to_sierra);
    let tx_execution_infos = txs_execution_outputs
        .into_iter()
        .map(|(execution_info, _)| execution_info.into())
        .collect();
    let transactions = txs_to_api_txs(txs);
    let old_block_number_and_hash =
        maybe_dummy_block_hash_and_number(block_info.block_number).map(|v| (v.number, v.hash));
    let os_block_input = OsBlockInput {
        contract_state_commitment_info: contracts_trie_commitment_info,
        contract_class_commitment_info: classes_trie_commitment_info,
        address_to_storage_commitment_info: storage_tries_commitment_infos,
        transactions,
        tx_execution_infos,
        declared_class_hash_to_component_hashes: declared_contracts_component_hashes,
        block_info,
        prev_block_hash: BlockHash(Felt::ZERO),
        new_block_hash: BlockHash(Felt::ZERO),
        old_block_number_and_hash,
    };
    os_block_inputs.push(os_block_input);
    cached_state_inputs.push(cached_state_input);

    let starknet_os_input = StarknetOsInput {
        os_block_inputs,
        cached_state_inputs,
        deprecated_compiled_classes: initial_state.deprecated_contracts.into_iter().collect(),
        compiled_classes: initial_state.contracts.into_iter().collect(),
    };
    let os_hints_config = OsHintsConfig { chain_info, ..Default::default() };
    let os_hints = OsHints { os_input: starknet_os_input, os_hints_config };
    let layout = LayoutName::all_cairo;
    run_os_stateless(layout, os_hints).unwrap()
}

/// Creates the commitment infos and the cached state input for the OS.
pub(crate) fn create_cached_state_input_and_commitment_infos(
    previous_contract_trie_root: HashOutput,
    new_contract_trie_root: HashOutput,
    previous_class_trie_root: HashOutput,
    new_class_trie_root: HashOutput,
    fact_storage: &MapStorage,
    keys: StateChangesKeys,
    class_hash_extension: Vec<ClassHash>,
) -> (CachedStateInput, CommitmentInfo, CommitmentInfo, HashMap<ContractAddress, CommitmentInfo>) {
    // TODO(Nimrod): Gather the keys from the state selector similarly to python.
    let mut leaf_indices: Vec<NodeIndex> = keys
        .modified_contracts
        .iter()
        .map(|address| NodeIndex::from_leaf_felt(&address.0))
        .collect();

    // Get previous contract state leaves.
    let sorted_leaf_indices = SortedLeafIndices::new(&mut leaf_indices);
    let config = OriginalSkeletonContractsTrieConfig {};
    let leaf_modifications = LeafModifications::new();
    let (_, previous_contract_states) = OriginalSkeletonTreeImpl::create_and_get_previous_leaves(
        fact_storage,
        previous_contract_trie_root,
        sorted_leaf_indices,
        &config,
        &leaf_modifications,
    )
    .unwrap();
    let (_, new_contract_states) = OriginalSkeletonTreeImpl::create_and_get_previous_leaves(
        fact_storage,
        new_contract_trie_root,
        sorted_leaf_indices,
        &config,
        &leaf_modifications,
    )
    .unwrap();
    let mut address_to_class_hash = HashMap::new();
    let mut address_to_nonce = HashMap::new();
    let mut address_to_previous_storage_root_hash = HashMap::new();
    let mut address_to_new_storage_root_hash = HashMap::new();
    for (idx, contract_state) in previous_contract_states {
        let address: ContractAddress =
            Felt::try_from(idx - NodeIndex::FIRST_LEAF).unwrap().try_into().unwrap();
        address_to_class_hash.insert(address, contract_state.class_hash);
        address_to_nonce.insert(address, contract_state.nonce);
        address_to_previous_storage_root_hash.insert(address, contract_state.storage_root_hash);
        address_to_new_storage_root_hash
            .insert(address, new_contract_states[&idx].storage_root_hash);
    }
    // Get previous class leaves.
    let mut leaf_indices: Vec<NodeIndex> = keys
        .compiled_class_hash_keys
        .iter()
        .chain(class_hash_extension.iter())
        .map(|address| NodeIndex::from_leaf_felt(&address.0))
        .collect();

    let sorted_leaf_indices = SortedLeafIndices::new(&mut leaf_indices);
    let compare_modified_leaves = false;
    let config = OriginalSkeletonClassesTrieConfig::new(compare_modified_leaves);
    let leaf_modifications = LeafModifications::new();
    let (_, previous_class_leaves) = OriginalSkeletonTreeImpl::create_and_get_previous_leaves(
        fact_storage,
        previous_class_trie_root,
        sorted_leaf_indices,
        &config,
        &leaf_modifications,
    )
    .unwrap();
    let class_hash_to_compiled_class_hash = previous_class_leaves
        .into_iter()
        .map(|(idx, v)| {
            (
                ClassHash(Felt::try_from(idx - NodeIndex::FIRST_LEAF).unwrap()),
                StarknetAPICompiledClassHash(v.0),
            )
        })
        .collect();

    let mut storage = HashMap::new();
    let config = OriginalSkeletonStorageTrieConfig::new(compare_modified_leaves);
    for address in keys.modified_contracts {
        let mut storage_keys_indices: Vec<NodeIndex> = keys
            .storage_keys
            .iter()
            .filter_map(|(add, key)| {
                if add == &address { Some(NodeIndex::from_leaf_felt(&key.0)) } else { None }
            })
            .collect();
        let sorted_leaf_indices = SortedLeafIndices::new(&mut storage_keys_indices);
        let leaf_modifications = LeafModifications::new();
        let (_, previous_storage_leaves) =
            OriginalSkeletonTreeImpl::create_and_get_previous_leaves(
                fact_storage,
                address_to_previous_storage_root_hash[&address],
                sorted_leaf_indices,
                &config,
                &leaf_modifications,
            )
            .unwrap();
        let previous_storage_leaves: HashMap<StorageKey, Felt> = previous_storage_leaves
            .into_iter()
            .map(|(idx, v)| {
                (Felt::try_from(idx - NodeIndex::FIRST_LEAF).unwrap().try_into().unwrap(), v.0)
            })
            .collect();
        storage.insert(address, previous_storage_leaves);
    }

    // TODO(Nimrod): Gather facts per tree.
    let facts = filter_inner_nodes_from_facts(&fact_storage.storage);
    let contracts_trie_commitment_info = CommitmentInfo {
        previous_root: previous_contract_trie_root,
        updated_root: new_contract_trie_root,
        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: facts.clone(),
    };
    let classes_trie_commitment_info = CommitmentInfo {
        previous_root: previous_class_trie_root,
        updated_root: new_class_trie_root,
        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
        commitment_facts: facts.clone(),
    };
    let storage_tries_commitment_infos = address_to_previous_storage_root_hash
        .iter()
        .map(|(address, previous_root_hash)| {
            (
                *address,
                CommitmentInfo {
                    previous_root: *previous_root_hash,
                    updated_root: address_to_new_storage_root_hash[address],
                    tree_height: SubTreeHeight::ACTUAL_HEIGHT,
                    commitment_facts: facts.clone(),
                },
            )
        })
        .collect();
    (
        CachedStateInput {
            storage,
            address_to_class_hash,
            address_to_nonce,
            class_hash_to_compiled_class_hash,
        },
        contracts_trie_commitment_info,
        classes_trie_commitment_info,
        storage_tries_commitment_infos,
    )
}

/// Filters inner nodes from the fact storage for the commitment info that will be passed to the OS.
/// Note: This produces many redundancy as the entire fact storage will be contained in each
/// commitment info
pub(crate) fn filter_inner_nodes_from_facts(
    fact_storage: &HashMap<DbKey, DbValue>,
) -> HashMap<HashOutput, Vec<Felt>> {
    let mut inner_nodes: HashMap<HashOutput, Vec<Felt>> = HashMap::new();
    let inner_node_prefix = DbKeyPrefix::from(PatriciaPrefix::InnerNode).to_bytes();
    for (key, value) in fact_storage.iter() {
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

fn get_declared_contracts_component_hashes(
    txs: &[Transaction],
    class_hash_to_sierra: &HashMap<ClassHash, SierraContractClass>,
) -> HashMap<ClassHash, ContractClassComponentHashes> {
    let mut declared_class_hash_to_component_hashes = HashMap::new();
    for tx in txs {
        if let Transaction::Account(AccountTransaction {
            tx: ExecutableTransaction::Declare(declare_tx),
            ..
        }) = tx
        {
            if let ContractClass::V1(_) = &declare_tx.class_info.contract_class {
                let class_hash = declare_tx.class_hash();
                let sierra_contract = class_hash_to_sierra
                    .get(&class_hash)
                    .expect("Expected a Sierra contract class for the declared class hash.");
                let mut contract_class_version = "CONTRACT_CLASS_V".to_string();
                contract_class_version.push_str(&sierra_contract.contract_class_version);
                let contract_class_version =
                    Felt::from_bytes_be_slice(contract_class_version.as_bytes());
                let component_hashes = sierra_contract.get_component_hashes(contract_class_version);
                declared_class_hash_to_component_hashes.insert(class_hash, component_hashes);
            }
        }
    }
    declared_class_hash_to_component_hashes
}

fn txs_to_api_txs(txs: Vec<Transaction>) -> Vec<StarknetApiTransaction> {
    txs.into_iter()
        .map(|tx| match tx {
            Transaction::Account(account_transaction) => {
                StarknetApiTransaction::Account(account_transaction.tx)
            }
            Transaction::L1Handler(l1_handler_transaction) => {
                StarknetApiTransaction::L1Handler(l1_handler_transaction)
            }
        })
        .collect()
}

/// Creates the initial state for the flow test which includes:
/// Declares token and account contracts.
/// Deploys both contracts and funds the account.
pub(crate) async fn create_default_initial_state<S: FlowTestState>() -> InitialStateData<S> {
    // Start from an empty state.
    let empty_state: InitialStateData<S> = InitialStateData::default();
    // Declare account and ERC20 contracts.
    let account_contract =
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let account_sierra = account_contract.get_sierra();
    let account_contract = account_contract.get_class();
    let ContractClass::V1((account_casm, _sierra_version)) = account_contract else {
        panic!("Expected a V1 contract class, but got: {:?}", account_contract);
    };
    let account_declare_tx = create_cairo1_declare_tx(
        &account_sierra,
        &account_casm,
        DeclareTransaction::bootstrap_address(),
        Nonce::default(),
        ValidResourceBounds::create_for_testing_no_fee_enforcement(),
    );
    let erc20_contract = FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let erc20_sierra = erc20_contract.get_sierra();
    let erc20_class = erc20_contract.get_class();
    let ContractClass::V1((erc20_casm, _sierra_version)) = erc20_class else {
        panic!("Expected a V1 contract class, but got: {:?}", erc20_class);
    };
    let erc20_declare_tx = create_cairo1_declare_tx(
        &erc20_sierra,
        &erc20_casm,
        DeclareTransaction::bootstrap_address(),
        Nonce::default(),
        ValidResourceBounds::create_for_testing_no_fee_enforcement(),
    );

    let mut txs = vec![
        Transaction::new_for_sequencing(StarknetApiTransaction::Account(account_declare_tx)),
        Transaction::new_for_sequencing(StarknetApiTransaction::Account(erc20_declare_tx)),
    ];

    // Deploy an account.
    let account_contract_class_hash = account_sierra.calculate_class_hash();
    let deploy_account_tx_args = deploy_account_tx_args! {
        constructor_calldata: Calldata(vec![].into()),
        class_hash: account_contract_class_hash,
    };
    let deploy_tx = deploy_account_tx(deploy_account_tx_args, Nonce(Felt::ZERO));
    let deploy_tx = DeployAccountTransaction::create(deploy_tx, &CHAIN_ID_FOR_TESTS).unwrap();
    let account_contract_address = deploy_tx.contract_address;
    let deploy_tx = Transaction::new_for_sequencing(StarknetApiTransaction::Account(
        ExecutableTransaction::DeployAccount(deploy_tx),
    ));
    txs.push(deploy_tx);

    // Deploy token contract using the deploy syscall.
    let erc20_class_hash = erc20_sierra.calculate_class_hash();
    let constructor_calldata = [
        9.into(),                          // constructor length
        9000.into(),                       // token name
        8000.into(),                       // token symbol
        10.into(),                         // token decimals
        100000000000000_u128.into(),       // initial supply lsb
        16.into(),                         // initial supply msb
        *account_contract_address.0.key(), // recipient address
        *account_contract_address.0.key(), // permitted minter
        *account_contract_address.0.key(), // provisional_governance_admin
        10.into(),                         // upgrade delay
    ];
    let contract_address_salt = Felt::ONE;
    let calldata: Vec<_> = [erc20_class_hash.0, contract_address_salt]
        .into_iter()
        .chain(constructor_calldata.into_iter())
        .collect();
    let deploy_contract_calldata = create_calldata(
        account_contract_address,
        DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME,
        &calldata,
    );
    let invoke_tx_args = invoke_tx_args! {
        sender_address: account_contract_address,
        nonce: Nonce(Felt::ONE),
        calldata: deploy_contract_calldata,

    };
    let deploy_contract_tx = invoke_tx(invoke_tx_args);
    let deploy_contract_tx =
        InvokeTransaction::create(deploy_contract_tx, &CHAIN_ID_FOR_TESTS).unwrap();
    let deploy_contract_tx = Transaction::new_for_sequencing(StarknetApiTransaction::Account(
        ExecutableTransaction::Invoke(deploy_contract_tx),
    ));
    txs.push(deploy_contract_tx);

    // Execute these 4 txs.
    let initial_state_reader = S::default();
    let (execution_outputs, summary, mut state_reader) =
        execute_transactions(initial_state_reader, &txs, BlockContext::create_for_testing());
    assert_eq!(execution_outputs.len(), 4, "Expected four transaction execution outputs.");
    // Make sure none of them is reverted.
    assert!(execution_outputs[0].0.revert_error.is_none());
    assert!(execution_outputs[1].0.revert_error.is_none());
    assert!(execution_outputs[2].0.revert_error.is_none());
    assert!(execution_outputs[3].0.revert_error.is_none());
    let fee_token_address =
        &execution_outputs[3].0.execute_call_info.as_ref().unwrap().execution.retdata.0[0];
    let fee_token_address = ContractAddress::try_from(*fee_token_address).unwrap();

    // Commit the new state and save the new facts.
    let committer_input = create_committer_input(
        summary.state_diff,
        &empty_state.fact_storage,
        empty_state.contracts_trie_root_hash,
        empty_state.classes_trie_root_hash,
    );
    let filled_forest =
        commit_block(committer_input).await.expect("Failed to commit the given block.");
    let mut fact_storage = MapStorage::default();
    filled_forest.write_to_storage(&mut fact_storage);
    let new_contracts_trie_root_hash = filled_forest.get_contract_root_hash();
    let new_classes_trie_root_hash = filled_forest.get_compiled_class_root_hash();
    // Update the state reader with the state diff.
    let state_diff = state_reader.to_state_diff().unwrap();
    state_reader
        .state
        .apply_writes(&state_diff.state_maps, &state_reader.class_hash_to_class.borrow());
    let class_hash_to_sierra = HashMap::from([
        (account_contract_class_hash, account_sierra),
        (erc20_class_hash, erc20_sierra),
    ]);

    let erc20_compiled_class_hash = StarknetAPICompiledClassHash(erc20_casm.compiled_class_hash());
    let account_contract_compiled_class_hash =
        StarknetAPICompiledClassHash(account_casm.compiled_class_hash());
    InitialStateData {
        updatable_state: state_reader.state,
        fact_storage,
        contracts_trie_root_hash: new_contracts_trie_root_hash,
        classes_trie_root_hash: new_classes_trie_root_hash,
        contracts: HashMap::from([
            (erc20_compiled_class_hash, erc20_casm),
            (account_contract_compiled_class_hash, account_casm),
        ]),
        deprecated_contracts: HashMap::new(),
        funded_account: account_contract_address,
        fee_token_address,
        class_hash_to_sierra,
    }
}

fn create_cairo1_declare_tx(
    sierra: &SierraContractClass,
    casm: &CasmContractClass,
    sender_address: ContractAddress,
    nonce: Nonce,
    resource_bounds: ValidResourceBounds,
) -> ExecutableTransaction {
    let class_hash = sierra.calculate_class_hash();
    let compiled_class_hash = StarknetAPICompiledClassHash(casm.compiled_class_hash());
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
    ExecutableTransaction::Declare(tx)
}

// Creates two txs: declare and deploy of the test contract. Updates the initial state to have the
// information needed for these txs.
pub(crate) fn poc_txs<S: FlowTestState>(
    initial_state: &mut InitialStateData<S>,
) -> [Transaction; 2] {
    // Declare a test contract.
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let test_contract_sierra = test_contract.get_sierra();
    let test_contract_class = test_contract.get_class();
    let ContractClass::V1((test_contract_casm, _sierra_version)) = test_contract_class.clone()
    else {
        panic!("Expected a V1 contract class, but got: {:?}", test_contract_class);
    };
    let resource_bounds = ResourceBounds {
        max_amount: GasAmount(u64::pow(10, 10)),
        max_price_per_unit: DEFAULT_STRK_L1_GAS_PRICE.into(),
    };
    let resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: resource_bounds,
        l2_gas: resource_bounds,
        l1_data_gas: resource_bounds,
    });
    let test_contract_declare_tx = create_cairo1_declare_tx(
        &test_contract_sierra,
        &test_contract_casm,
        initial_state.funded_account,
        Nonce(Felt::TWO),
        resource_bounds,
    );
    let class_hash = test_contract_sierra.calculate_class_hash();
    let compiled_class_hash =
        StarknetAPICompiledClassHash(test_contract_casm.compiled_class_hash());
    initial_state.contracts.insert(compiled_class_hash, test_contract_casm);
    let state_maps = StateMaps {
        compiled_class_hashes: HashMap::from([(class_hash, compiled_class_hash)]),
        ..Default::default()
    };
    initial_state.updatable_state.apply_writes(&state_maps, &HashMap::new());
    initial_state.class_hash_to_sierra.insert(class_hash, test_contract_sierra);

    // Deploy the test contract using the deploy syscall.
    let constructor_calldata = [
        2.into(),  // constructor length
        7.into(),  // arg1
        90.into(), // arg2
    ];
    let contract_address_salt = Felt::ONE;
    let calldata: Vec<_> =
        [class_hash.0, contract_address_salt].into_iter().chain(constructor_calldata).collect();
    let deploy_contract_calldata = create_calldata(
        initial_state.funded_account,
        DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME,
        &calldata,
    );
    let invoke_tx_args = invoke_tx_args! {
        sender_address: initial_state.funded_account,
        nonce: Nonce(Felt::THREE),
        calldata: deploy_contract_calldata,
        resource_bounds,
    };
    let deploy_contract_tx = invoke_tx(invoke_tx_args);
    let deploy_contract_tx =
        InvokeTransaction::create(deploy_contract_tx, &CHAIN_ID_FOR_TESTS).unwrap();
    [
        Transaction::new_for_sequencing(StarknetApiTransaction::Account(test_contract_declare_tx)),
        Transaction::new_for_sequencing(StarknetApiTransaction::Account(
            ExecutableTransaction::Invoke(deploy_contract_tx),
        )),
    ]
}

pub(crate) trait FlowTestState: UpdatableState + Default + Sync + Send + 'static {}

impl FlowTestState for DictStateReader {}

type MultiBlockTransactions = Vec<(BlockContext, Vec<OsFlowTestTransaction>)>;

// pub(crate) trait OsTestScenario<S: FlowTestState> {
//     /// Creates the initial state for the OS test to start from.
//     async fn create_initial_state() -> InitialStateData<S> {
//         // Default implementation. Can be overridden.
//         create_default_initial_state().await
//     }
//     /// Returns the transactions in a multi-block structure to be executed in
//     /// the OS test.
//     fn transactions(&self) -> MultiBlockTransactions;

//     /// Perform validation on the os output.
//     fn validate_post_execution(&self, os_output: StarknetOsRunnerOutput);

//     /// Executes the test scenario.
//     async fn execute_scenario(&self) {
//         let initial_state = Self::create_initial_state().await;
//         let txs = self.transactions();
//         let os_output = flow_test_body(initial_state, txs).await;
//         self.validate_post_execution(os_output);
//     }

//     /// A util to allow creation a multi-block transactions structure easily from the flat
//     /// transactions by dividing them as equally as possible into a multi-block. The block
//     /// context of each block is created from the  given block range.
//     fn create_transactions_from_flat_transactions(
//         txs: Vec<Transaction>,
//         n_blocks_in_multi_block: u64,
//         initial_block_number: u64,
//     ) -> MultiBlockTransactions { todo!()
//     }
// }

pub(crate) enum OsFlowTestTransaction {
    Account(OsFlowTestAccountTransaction),
    L1Handler(L1HandlerTransaction),
}

pub(crate) enum OsFlowTestAccountTransaction {
    // Cairo 1 declare must supply the sierra. It's optional to allow cairo 0 declaration.
    Declare(DeclareTransaction, Option<SierraContractClass>),
    DeployAccount(DeployAccountTransaction),
    Invoke(InvokeTransaction),
}

impl From<OsFlowTestAccountTransaction> for AccountTransaction {
    fn from(tx: OsFlowTestAccountTransaction) -> Self {
        let tx = match tx {
            OsFlowTestAccountTransaction::Declare(declare_transaction, _sierra_contract_class) => {
                ExecutableTransaction::Declare(declare_transaction)
            }
            OsFlowTestAccountTransaction::DeployAccount(deploy_account_transaction) => {
                ExecutableTransaction::DeployAccount(deploy_account_transaction)
            }
            OsFlowTestAccountTransaction::Invoke(invoke_transaction) => {
                ExecutableTransaction::Invoke(invoke_transaction)
            }
        };
        AccountTransaction::new_for_sequencing(tx)
    }
}

impl From<OsFlowTestTransaction> for Transaction {
    fn from(tx: OsFlowTestTransaction) -> Self {
        match tx {
            OsFlowTestTransaction::Account(account_tx) => Transaction::Account(account_tx.into()),
            OsFlowTestTransaction::L1Handler(l1_handler_tx) => {
                Transaction::new_for_sequencing(StarknetApiTransaction::L1Handler(l1_handler_tx))
            }
        }
    }
}
