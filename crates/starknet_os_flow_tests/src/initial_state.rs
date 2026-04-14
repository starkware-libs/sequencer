use std::collections::{BTreeMap, HashMap, HashSet};

use apollo_integration_tests::state_reader::{
    proof_flow_integration_chain_info,
    proof_flow_integration_genesis_data,
    ProofFlowGenesisClasses,
};
use apollo_integration_tests::utils::create_proof_flow_tx_generator;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::cached_state::{ContractClassMapping, StateMaps};
use blockifier::state::state_api::UpdatableState;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::generate_block_hash_storage_updates;
use blockifier::transaction::transaction_execution::Transaction;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::executable_transaction::{
    AccountTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    Transaction as StarknetAPITransaction,
};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcTransaction};
use starknet_api::state::{ContractClassComponentHashes, SierraContractClass, ThinStateDiff};
use starknet_api::test_utils::deploy_account::deploy_account_tx;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::test_utils::{NonceManager, CHAIN_ID_FOR_TESTS, CURRENT_BLOCK_NUMBER};
use starknet_api::transaction::constants::DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt, ValidResourceBounds};
use starknet_api::{calldata, deploy_account_tx_args, invoke_tx_args};
use starknet_committer::block_committer::input::StateDiff;
use starknet_committer::db::facts_db::FactsDb;
use starknet_committer::db::forest_trait::StorageInitializer;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_transaction_prover::running::committer_utils::{
    commit_state_diff,
    state_maps_to_committer_state_diff,
};
use starknet_types_core::felt::Felt;

use crate::test_manager::{
    block_context_for_flow_tests,
    EXPECTED_STRK_FEE_TOKEN_ADDRESS,
    FUNDED_ACCOUNT_ADDRESS,
    STRK_FEE_TOKEN_ADDRESS,
};
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;
use crate::utils::{
    create_cairo1_bootstrap_declare_tx,
    create_declare_tx,
    execute_transactions,
    get_class_hash_of_feature_contract,
    ExecutionOutput,
};

const INITIAL_TOKEN_SUPPLY: u128 = 10_000_000_000_000_000_000_000_000_000_000_000;
const STRK_TOKEN_NAME: &[u8] = b"StarkNet Token";
const STRK_SYMBOL: &[u8] = b"STRK";
const STRK_DECIMALS: u8 = 18;

/// Trait alias for state readers used in flow tests.
pub(crate) trait FlowTestState: Default + UpdatableState + Send {}
impl<S: Default + UpdatableState + Send> FlowTestState for S {}

/// Gathers the information needed to execute a flow test.
pub(crate) struct InitialStateData<S: FlowTestState> {
    pub(crate) initial_state: InitialState<S>,
    pub(crate) nonce_manager: NonceManager,
    pub(crate) execution_contracts: OsExecutionContracts,
}

#[derive(Default)]
pub(crate) struct OsExecutionContracts {
    // Cairo contracts that are executed during the OS execution.
    pub(crate) executed: ExecutedContracts,
    // Cairo 1 contracts that are declared during the OS execution.
    pub(crate) declared_class_hash_to_component_hashes:
        HashMap<ClassHash, ContractClassComponentHashes>,
}

impl OsExecutionContracts {
    pub(crate) fn add_cairo1_contract(
        &mut self,
        casm_contract_class: CasmContractClass,
        sierra: &SierraContractClass,
    ) {
        self.executed.add_cairo1_contract(casm_contract_class);
        self.declared_class_hash_to_component_hashes
            .insert(sierra.calculate_class_hash(), sierra.get_component_hashes());
    }

    pub(crate) fn add_deprecated_contract(
        &mut self,
        class_hash: ClassHash,
        deprecated_contract_class: DeprecatedContractClass,
    ) {
        self.executed.add_deprecated_contract(class_hash, deprecated_contract_class);
    }
}

#[derive(Default)]
pub(crate) struct ExecutedContracts {
    pub(crate) contracts: BTreeMap<CompiledClassHash, CasmContractClass>,
    pub(crate) deprecated_contracts: BTreeMap<ClassHash, DeprecatedContractClass>,
}

impl ExecutedContracts {
    pub(crate) fn add_cairo1_contract(&mut self, casm_contract_class: CasmContractClass) {
        self.contracts.insert(casm_contract_class.hash(&HashVersion::V2), casm_contract_class);
    }

    pub(crate) fn add_deprecated_contract(
        &mut self,
        class_hash: ClassHash,
        deprecated_contract_class: DeprecatedContractClass,
    ) {
        self.deprecated_contracts.insert(class_hash, deprecated_contract_class);
    }
}

pub(crate) struct InitialState<S: FlowTestState> {
    pub(crate) updatable_state: S,
    pub(crate) commitment_storage: MapStorage,
    // Current patricia roots.
    pub(crate) contracts_trie_root_hash: HashOutput,
    pub(crate) classes_trie_root_hash: HashOutput,
    // Block context of the last block in the initial state.
    pub(crate) block_context: BlockContext,
}

/// Creates the initial state for the flow test which includes:
/// Declares token and account contracts.
/// Deploys both contracts and funds the account.
/// Also deploys extra contracts as requested (and declares them if they are not already declared).
pub(crate) async fn create_default_initial_state_data<S: FlowTestState, const N: usize>(
    extra_contracts: [(FeatureContract, Calldata); N],
) -> (InitialStateData<S>, [ContractAddress; N]) {
    let (
        InitialTransactionsData {
            transactions: default_initial_state_txs,
            execution_contracts,
            nonce_manager,
        },
        extra_contracts_addresses,
    ) = create_default_initial_state_txs_and_contracts(extra_contracts);
    // Execute these 4 txs.
    let initial_state_reader = S::default();
    let initial_block_number = BlockNumber(CURRENT_BLOCK_NUMBER);
    let use_kzg_da = false;
    let block_context = block_context_for_flow_tests(initial_block_number, use_kzg_da);
    let virtual_os = false;
    let ExecutionOutput { execution_outputs, mut final_state } = execute_transactions(
        initial_state_reader,
        &default_initial_state_txs,
        block_context.clone(),
        virtual_os,
    );
    assert_eq!(
        execution_outputs.len(),
        default_initial_state_txs.len(),
        "Expected {} transaction execution outputs.",
        default_initial_state_txs.len()
    );
    // Make sure none of them is reverted.
    assert!(execution_outputs.iter().all(|output| output.0.revert_error.is_none()));
    // Update the state reader with the state diff.
    let mut state_diff = final_state.to_state_diff().unwrap().state_maps;
    // Sanity check to verify the STRK_FEE_TOKEN_ADDRESS constant.
    assert_eq!(
        state_diff.class_hashes[&STRK_FEE_TOKEN_ADDRESS],
        FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm))
            .get_sierra()
            .calculate_class_hash()
    );
    // Add historical block hashes to state_diff for commitment.
    let block_hash_state_maps = generate_block_hash_storage_updates();
    state_diff.extend(&block_hash_state_maps);

    final_state.state.apply_writes(&state_diff, &final_state.class_hash_to_class.borrow());

    // Commits the state diff with block hash mappings.
    let committer_state_diff = state_maps_to_committer_state_diff(state_diff);
    let (commitment_output, commitment_storage) =
        commit_initial_state_diff(committer_state_diff).await;

    let initial_state = InitialState {
        updatable_state: final_state.state,
        commitment_storage,
        contracts_trie_root_hash: commitment_output.contracts_trie_root_hash,
        classes_trie_root_hash: commitment_output.classes_trie_root_hash,
        block_context,
    };

    (
        InitialStateData { initial_state, nonce_manager, execution_contracts },
        extra_contracts_addresses,
    )
}

struct InitialTransactionsData {
    pub(crate) transactions: Vec<Transaction>,
    pub(crate) execution_contracts: OsExecutionContracts,
    pub(crate) nonce_manager: NonceManager,
}

fn create_default_initial_state_txs_and_contracts<const N: usize>(
    extra_contracts: [(FeatureContract, Calldata); N],
) -> (InitialTransactionsData, [ContractAddress; N]) {
    let mut os_execution_contracts = OsExecutionContracts::default();
    // Declare account and ERC20 contracts.
    let account_contract =
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let account_declare_tx =
        create_cairo1_bootstrap_declare_tx(account_contract, &mut os_execution_contracts);
    let erc20_contract = FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let erc20_declare_tx =
        create_cairo1_bootstrap_declare_tx(erc20_contract, &mut os_execution_contracts);

    let mut txs = vec![
        Transaction::new_for_sequencing(StarknetAPITransaction::Account(account_declare_tx)),
        Transaction::new_for_sequencing(StarknetAPITransaction::Account(erc20_declare_tx)),
    ];
    let mut nonce_manager = NonceManager::default();

    // Deploy an account.
    let deploy_tx = get_initial_deploy_account_tx();
    let account_contract_address = deploy_tx.contract_address;
    // Sanity check to verify the FUNDED_ACCOUNT_ADDRESS constant.
    assert_eq!(account_contract_address, *FUNDED_ACCOUNT_ADDRESS);
    // Update the nonce.
    nonce_manager.next(*FUNDED_ACCOUNT_ADDRESS);
    let deploy_tx = Transaction::new_for_sequencing(StarknetAPITransaction::Account(
        AccountTransaction::DeployAccount(deploy_tx),
    ));
    txs.push(deploy_tx);

    // Deploy token contract using the deploy syscall.
    let nonce = nonce_manager.next(account_contract_address);
    let (deploy_contract_tx, _) = get_deploy_fee_token_tx_and_address(nonce);
    txs.push(deploy_contract_tx);

    // Deploy extra contracts. Declare contracts that are not already declared.
    let mut declared_contracts = HashSet::from([account_contract, erc20_contract]);
    let mut extra_addresses = Vec::new();
    for (contract, calldata) in extra_contracts {
        if !declared_contracts.contains(&contract) {
            // Add a declare transaction for the contract.
            // No need for bootstrap mode: funded account already exists at this point.
            txs.push(Transaction::new_for_sequencing(StarknetAPITransaction::Account(
                create_declare_tx(contract, &mut nonce_manager, &mut os_execution_contracts, false),
            )));
            declared_contracts.insert(contract);
        }
        // Deploy.
        let (deploy_tx, address) = get_deploy_contract_tx_and_address(
            get_class_hash_of_feature_contract(contract),
            calldata,
            nonce_manager.next(account_contract_address),
            *NON_TRIVIAL_RESOURCE_BOUNDS,
        );
        txs.push(deploy_tx);
        extra_addresses.push(address);
    }

    (
        InitialTransactionsData {
            transactions: txs,
            execution_contracts: os_execution_contracts,
            nonce_manager,
        },
        extra_addresses.try_into().unwrap(),
    )
}

pub(crate) async fn commit_initial_state_diff(
    committer_state_diff: StateDiff,
) -> (StateRoots, MapStorage) {
    let mut facts_db = FactsDb::new(MapStorage::default());
    let classes_trie_root = HashOutput::ROOT_OF_EMPTY_TREE;
    let contract_trie_root = HashOutput::ROOT_OF_EMPTY_TREE;
    let state_roots = commit_state_diff(
        &mut facts_db,
        contract_trie_root,
        classes_trie_root,
        committer_state_diff,
    )
    .await
    .expect("Failed to commit initial state diff.");
    (state_roots, facts_db.consume_storage())
}

/// Creates the initial state to match the proof flow integration test.
/// Returns the state data and the genesis `ThinStateDiff` for `state_diff_commitment` overriding.
pub(crate) async fn create_proof_flow_integration_initial_state()
-> (InitialStateData<DictStateReader>, ThinStateDiff) {
    let chain_info = proof_flow_integration_chain_info();
    let (thin, classes) = proof_flow_integration_genesis_data(&chain_info);
    let class_hash_to_class = contract_class_mapping_from_proof_flow_genesis_classes(&classes);

    let mut reader = DictStateReader::default();
    for (class_hash, (sierra, _casm)) in &classes.cairo1_contract_classes {
        reader.class_hash_to_sierra.insert(*class_hash, sierra.clone());
    }

    let state_maps = thin_state_diff_to_state_maps(&thin);
    reader.apply_writes(&state_maps, &class_hash_to_class);

    let committer_state_diff = state_maps_to_committer_state_diff(state_maps);
    let (commitment_output, commitment_storage) =
        commit_initial_state_diff(committer_state_diff).await;

    let initial_block_number = BlockNumber(CURRENT_BLOCK_NUMBER);
    let use_kzg_da = false;
    let block_context = block_context_for_flow_tests(initial_block_number, use_kzg_da);

    let execution_contracts = os_execution_contracts_from_proof_flow_genesis_classes(&classes);

    let initial_state = InitialState {
        updatable_state: reader,
        commitment_storage,
        contracts_trie_root_hash: commitment_output.contracts_trie_root_hash,
        classes_trie_root_hash: commitment_output.classes_trie_root_hash,
        block_context,
    };

    (
        InitialStateData {
            initial_state,
            nonce_manager: NonceManager::default(),
            execution_contracts,
        },
        thin,
    )
}

fn thin_state_diff_to_state_maps(thin: &ThinStateDiff) -> StateMaps {
    let mut storage = HashMap::new();
    for (addr, entries) in &thin.storage_diffs {
        for (key, val) in entries {
            storage.insert((*addr, *key), *val);
        }
    }
    let declared_contracts: HashMap<ClassHash, bool> =
        thin.deprecated_declared_classes.iter().map(|ch| (*ch, true)).collect();
    StateMaps {
        nonces: thin.nonces.iter().map(|(k, v)| (*k, *v)).collect(),
        class_hashes: thin.deployed_contracts.iter().map(|(k, v)| (*k, *v)).collect(),
        storage,
        compiled_class_hashes: thin
            .class_hash_to_compiled_class_hash
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect(),
        declared_contracts,
    }
}

fn contract_class_mapping_from_proof_flow_genesis_classes(
    classes: &ProofFlowGenesisClasses,
) -> ContractClassMapping {
    let mut m = ContractClassMapping::new();
    for (class_hash, (sierra, casm)) in &classes.cairo1_contract_classes {
        let sierra_version = sierra.get_sierra_version().unwrap();
        let contract_class = ContractClass::V1((casm.clone(), sierra_version));
        m.insert(
            *class_hash,
            RunnableCompiledClass::try_from(contract_class).expect("cairo1 class"),
        );
    }
    m
}

fn os_execution_contracts_from_proof_flow_genesis_classes(
    classes: &ProofFlowGenesisClasses,
) -> OsExecutionContracts {
    let mut execution_contracts = OsExecutionContracts::default();
    for (_class_hash, (sierra, casm)) in &classes.cairo1_contract_classes {
        execution_contracts.add_cairo1_contract(casm.clone(), sierra);
    }
    execution_contracts
}

/// Block 0 invoke executable (proof-flow integration: account deployed in genesis).
pub(crate) fn proof_flow_integration_block_0_executable_transactions() -> InvokeTransaction {
    let mut tx_generator = create_proof_flow_tx_generator();
    let rpc_tx = tx_generator.account_with_id_mut(0).generate_trivial_rpc_invoke_tx(1);
    match rpc_tx {
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(rpc)) => InvokeTransaction::create(
            starknet_api::transaction::InvokeTransaction::V3(rpc.clone().into()),
            &CHAIN_ID_FOR_TESTS,
        )
        .expect("invoke executable"),
        _ => panic!("expected invoke RPC tx, got {:?}", rpc_tx),
    }
}

pub(crate) fn get_initial_deploy_account_tx() -> DeployAccountTransaction {
    let deploy_account_tx_args = deploy_account_tx_args! {
        class_hash: FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm)).get_sierra().calculate_class_hash(),
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
    };
    let deploy_tx = deploy_account_tx(deploy_account_tx_args, Nonce::default());
    DeployAccountTransaction::create(deploy_tx, &CHAIN_ID_FOR_TESTS).unwrap()
}

/// Creates a deploy-contract tx (from the funded account) and returns the tx and the expected
/// contract address.
pub(crate) fn get_deploy_contract_tx_and_address(
    class_hash: ClassHash,
    ctor_calldata: Calldata,
    nonce: Nonce,
    resource_bounds: ValidResourceBounds,
) -> (Transaction, ContractAddress) {
    let (deploy_contract_tx, contract_address) = get_deploy_contract_tx_and_address_with_salt(
        class_hash,
        ctor_calldata,
        nonce,
        resource_bounds,
        // Use the nonce as the salt so it's easy to deploy the same contract (with the same
        // constructor calldata) multiple times.
        ContractAddressSalt(nonce.0),
    );
    (
        Transaction::new_for_sequencing(StarknetAPITransaction::Account(
            AccountTransaction::Invoke(deploy_contract_tx),
        )),
        contract_address,
    )
}

pub(crate) fn get_deploy_contract_tx_and_address_with_salt(
    class_hash: ClassHash,
    ctor_calldata: Calldata,
    nonce: Nonce,
    resource_bounds: ValidResourceBounds,
    contract_address_salt: ContractAddressSalt,
) -> (InvokeTransaction, ContractAddress) {
    let calldata = [class_hash.0, contract_address_salt.0, ctor_calldata.0.len().into()]
        .iter()
        .chain(ctor_calldata.0.iter())
        .cloned()
        .collect::<Vec<Felt>>();

    let deploy_contract_calldata = create_calldata(
        *FUNDED_ACCOUNT_ADDRESS,
        DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME,
        &calldata,
    );

    let invoke_tx_args = invoke_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        nonce,
        calldata: deploy_contract_calldata,
        resource_bounds,
    };
    let deploy_contract_tx = invoke_tx(invoke_tx_args);
    let deploy_contract_tx =
        InvokeTransaction::create(deploy_contract_tx, &CHAIN_ID_FOR_TESTS).unwrap();

    let contract_address = calculate_contract_address(
        contract_address_salt,
        class_hash,
        &ctor_calldata,
        *FUNDED_ACCOUNT_ADDRESS,
    )
    .unwrap();
    (deploy_contract_tx, contract_address)
}
pub(crate) fn get_deploy_fee_token_tx_and_address(nonce: Nonce) -> (Transaction, ContractAddress) {
    let class_hash = FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm))
        .get_sierra()
        .calculate_class_hash();

    let constructor_calldata = calldata![
        Felt::from_bytes_be_slice(STRK_TOKEN_NAME),
        Felt::from_bytes_be_slice(STRK_SYMBOL),
        STRK_DECIMALS.into(),
        INITIAL_TOKEN_SUPPLY.into(),     // initial supply lsb
        0.into(),                        // initial supply msb
        *FUNDED_ACCOUNT_ADDRESS.0.key(), // recipient address
        *FUNDED_ACCOUNT_ADDRESS.0.key(), // permitted minter
        *FUNDED_ACCOUNT_ADDRESS.0.key(), // provisional_governance_admin
        10.into()                        // upgrade delay
    ];
    let (tx, address) = get_deploy_contract_tx_and_address(
        class_hash,
        constructor_calldata,
        nonce,
        ValidResourceBounds::create_for_testing_no_fee_enforcement(),
    );
    EXPECTED_STRK_FEE_TOKEN_ADDRESS.assert_debug_eq(&**address);
    (tx, address)
}
