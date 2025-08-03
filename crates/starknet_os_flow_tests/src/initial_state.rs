#![allow(dead_code)]
use std::collections::HashMap;

use blockifier::context::BlockContext;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::transaction::transaction_execution::Transaction;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
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
use starknet_api::state::{ContractClassComponentHashes, SierraContractClass, API_VERSION};
use starknet_api::test_utils::deploy_account::deploy_account_tx;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::test_utils::{NonceManager, CHAIN_ID_FOR_TESTS};
use starknet_api::transaction::constants::DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt, ValidResourceBounds};
use starknet_api::{deploy_account_tx_args, invoke_tx_args};
use starknet_committer::block_committer::input::StateDiff;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::map_storage::{BorrowedMapStorage, MapStorage};
use starknet_types_core::felt::Felt;

use crate::state_trait::FlowTestState;
use crate::test_manager::{FUNDED_ACCOUNT_ADDRESS, STRK_FEE_TOKEN_ADDRESS};
use crate::utils::{
    commit_state_diff,
    create_cairo1_bootstrap_declare_tx,
    create_committer_state_diff,
    execute_transactions,
    CommitmentOutput,
    ExecutionOutput,
};

const INITIAL_TOKEN_SUPPLY: u128 = 10_000_000_000_000_000_000_000_000_000_000_000;
const STRK_TOKEN_NAME: &[u8] = b"StarkNet Token";
const STRK_SYMBOL: &[u8] = b"STRK";
const STRK_DECIMALS: u8 = 18;

/// Gathers the information needed to execute a flow test.
pub(crate) struct InitialStateData<S: FlowTestState> {
    pub(crate) initial_state: InitialState<S>,
    pub(crate) execution_contracts: OsExecutionContracts,
}

#[derive(Default)]
pub(crate) struct OsExecutionContracts {
    // Cairo contracts that are executed during the OS execution.
    pub(crate) executed_contracts: ExecutedContracts,
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
        self.executed_contracts.add_cairo1_contract(casm_contract_class);
        self.declared_class_hash_to_component_hashes
            .insert(sierra.calculate_class_hash(), sierra.get_component_hashes(*API_VERSION));
    }

    pub(crate) fn add_deprecated_contract(
        &mut self,
        compiled_class_hash: CompiledClassHash,
        deprecated_contract_class: DeprecatedContractClass,
    ) {
        self.executed_contracts
            .add_deprecated_contract(compiled_class_hash, deprecated_contract_class);
    }
}

#[derive(Default)]
pub(crate) struct ExecutedContracts {
    pub(crate) contracts: HashMap<CompiledClassHash, CasmContractClass>,
    pub(crate) deprecated_contracts: HashMap<CompiledClassHash, DeprecatedContractClass>,
}

impl ExecutedContracts {
    pub(crate) fn add_cairo1_contract(&mut self, casm_contract_class: CasmContractClass) {
        self.contracts.insert(
            CompiledClassHash(casm_contract_class.compiled_class_hash()),
            casm_contract_class,
        );
    }

    pub(crate) fn add_deprecated_contract(
        &mut self,
        compiled_class_hash: CompiledClassHash,
        deprecated_contract_class: DeprecatedContractClass,
    ) {
        self.deprecated_contracts.insert(compiled_class_hash, deprecated_contract_class);
    }
}

pub(crate) struct InitialState<S: FlowTestState> {
    pub(crate) updatable_state: S,
    pub(crate) commitment_storage: MapStorage,
    // Current patricia roots.
    pub(crate) contracts_trie_root_hash: HashOutput,
    pub(crate) classes_trie_root_hash: HashOutput,
}

/// Creates the initial state for the flow test which includes:
/// Declares token and account contracts.
/// Deploys both contracts and funds the account.
pub(crate) async fn create_default_initial_state_data<S: FlowTestState>()
-> (InitialStateData<S>, NonceManager) {
    let InitialTransactionsData {
        transactions: default_initial_state_txs,
        execution_contracts,
        nonce_manager,
        token_deploy_index,
    } = create_default_initial_state_txs_and_contracts();
    // Execute these 4 txs.
    let initial_state_reader = S::create_empty_state();
    let ExecutionOutput { execution_outputs, block_summary, mut final_state } =
        execute_transactions(
            initial_state_reader,
            &default_initial_state_txs,
            BlockContext::create_for_testing(),
        );
    assert_eq!(
        execution_outputs.len(),
        default_initial_state_txs.len(),
        "Expected {} transaction execution outputs.",
        default_initial_state_txs.len()
    );
    // Make sure none of them is reverted.
    assert!(execution_outputs.iter().all(|output| output.0.revert_error.is_none()));
    let fee_token_address = ContractAddress::try_from(
        execution_outputs[token_deploy_index]
            .0
            .execute_call_info
            .as_ref()
            .unwrap()
            .execution
            .retdata
            .0[0],
    )
    .unwrap();
    // Update the state reader with the state diff.
    let state_diff = final_state.to_state_diff().unwrap();
    final_state
        .state
        .apply_writes(&state_diff.state_maps, &final_state.class_hash_to_class.borrow());

    // Sanity check to verify the STRK_FEE_TOKEN_ADDRESS constant.
    assert_eq!(fee_token_address, *STRK_FEE_TOKEN_ADDRESS);
    // Commit the state diff.
    let committer_state_diff = create_committer_state_diff(block_summary.state_diff);
    let (commitment_output, commitment_storage) =
        commit_initial_state_diff(committer_state_diff).await;

    let initial_state = InitialState {
        updatable_state: final_state.state,
        commitment_storage,
        contracts_trie_root_hash: commitment_output.contracts_trie_root_hash,
        classes_trie_root_hash: commitment_output.classes_trie_root_hash,
    };

    (InitialStateData { initial_state, execution_contracts }, nonce_manager)
}

struct InitialTransactionsData {
    pub(crate) transactions: Vec<Transaction>,
    pub(crate) execution_contracts: OsExecutionContracts,
    pub(crate) nonce_manager: NonceManager,
    pub(crate) token_deploy_index: usize,
}

fn create_default_initial_state_txs_and_contracts() -> InitialTransactionsData {
    let mut os_execution_contracts = OsExecutionContracts::default();
    // Declare account and ERC20 contracts.
    let account_contract =
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let account_sierra = account_contract.get_sierra();
    let account_contract = account_contract.get_class();
    let ContractClass::V1((account_casm, _sierra_version)) = account_contract else {
        panic!("Expected a V1 contract class, but got: {account_contract:?}");
    };
    let account_declare_tx = create_cairo1_bootstrap_declare_tx(
        &account_sierra,
        account_casm,
        &mut os_execution_contracts,
    );
    let erc20_contract = FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let erc20_sierra = erc20_contract.get_sierra();
    let erc20_class = erc20_contract.get_class();
    let ContractClass::V1((erc20_casm, _sierra_version)) = erc20_class else {
        panic!("Expected a V1 contract class, but got: {erc20_class:?}");
    };
    let erc20_declare_tx =
        create_cairo1_bootstrap_declare_tx(&erc20_sierra, erc20_casm, &mut os_execution_contracts);

    let mut txs = vec![
        Transaction::new_for_sequencing(StarknetAPITransaction::Account(account_declare_tx)),
        Transaction::new_for_sequencing(StarknetAPITransaction::Account(erc20_declare_tx)),
    ];
    let mut nonce_manager = NonceManager::default();

    // Deploy an account.
    let deploy_tx = get_deploy_account_tx();
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
    let (deploy_contract_tx, _) = get_fee_token_tx_and_address(nonce);
    txs.push(deploy_contract_tx);
    InitialTransactionsData {
        transactions: txs,
        execution_contracts: os_execution_contracts,
        nonce_manager,
        token_deploy_index: 3,
    }
}

pub(crate) async fn commit_initial_state_diff(
    committer_state_diff: StateDiff,
) -> (CommitmentOutput, MapStorage) {
    let mut map_storage = MapStorage::new();
    let mut borrowed_map_storage = BorrowedMapStorage { storage: &mut map_storage };
    let classes_trie_root = HashOutput::ROOT_OF_EMPTY_TREE;
    let contract_trie_root = HashOutput::ROOT_OF_EMPTY_TREE;
    (
        commit_state_diff(
            &mut borrowed_map_storage,
            contract_trie_root,
            classes_trie_root,
            committer_state_diff,
        )
        .await,
        map_storage,
    )
}

pub(crate) fn get_deploy_account_tx() -> DeployAccountTransaction {
    let deploy_account_tx_args = deploy_account_tx_args! {
        class_hash: FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm)).get_sierra().calculate_class_hash(),
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
    };
    let deploy_tx = deploy_account_tx(deploy_account_tx_args, Nonce::default());
    DeployAccountTransaction::create(deploy_tx, &CHAIN_ID_FOR_TESTS).unwrap()
}

pub(crate) fn get_fee_token_tx_and_address(nonce: Nonce) -> (Transaction, ContractAddress) {
    let class_hash = FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm))
        .get_sierra()
        .calculate_class_hash();
    let contract_address_salt = Felt::ONE;

    let constructor_calldata = [
        9.into(), // constructor length
        Felt::from_bytes_be_slice(STRK_TOKEN_NAME),
        Felt::from_bytes_be_slice(STRK_SYMBOL),
        STRK_DECIMALS.into(),
        INITIAL_TOKEN_SUPPLY.into(),     // initial supply lsb
        0.into(),                        // initial supply msb
        *FUNDED_ACCOUNT_ADDRESS.0.key(), // recipient address
        *FUNDED_ACCOUNT_ADDRESS.0.key(), // permitted minter
        *FUNDED_ACCOUNT_ADDRESS.0.key(), // provisional_governance_admin
        10.into(),                       // upgrade delay
    ];

    let calldata: Vec<_> =
        [class_hash.0, contract_address_salt].into_iter().chain(constructor_calldata).collect();

    let deploy_contract_calldata = create_calldata(
        *FUNDED_ACCOUNT_ADDRESS,
        DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME,
        &calldata,
    );

    let invoke_tx_args = invoke_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        nonce,
        calldata: deploy_contract_calldata,
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement()
    };
    let deploy_contract_tx = invoke_tx(invoke_tx_args);
    let deploy_contract_tx =
        InvokeTransaction::create(deploy_contract_tx, &CHAIN_ID_FOR_TESTS).unwrap();

    let contract_address = calculate_contract_address(
        ContractAddressSalt(contract_address_salt),
        class_hash,
        &Calldata(constructor_calldata[1..].to_vec().into()), // Ignore the length.
        *FUNDED_ACCOUNT_ADDRESS,
    )
    .unwrap();
    (
        Transaction::new_for_sequencing(StarknetAPITransaction::Account(
            AccountTransaction::Invoke(deploy_contract_tx),
        )),
        contract_address,
    )
}
