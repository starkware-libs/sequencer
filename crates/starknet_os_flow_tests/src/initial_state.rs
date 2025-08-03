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
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::executable_transaction::{
    AccountTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    Transaction as StarknetAPITransaction,
};
use starknet_api::state::{ContractClassComponentHashes, API_VERSION};
use starknet_api::test_utils::deploy_account::deploy_account_tx;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::test_utils::{NonceManager, CHAIN_ID_FOR_TESTS};
use starknet_api::transaction::constants::DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::Calldata;
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

pub(crate) const INITIAL_TOKEN_SUPPLY: u128 = 10_000_000_000_000_000_000;

/// Gathers the information needed to execute a flow test.
pub(crate) struct InitialStateData<S: FlowTestState> {
    pub(crate) initial_state: InitialState<S>,
    pub(crate) execution_contracts: OsExecutionContracts,
}

pub(crate) struct OsExecutionContracts {
    // Cairo contracts that are executed during the OS execution.
    pub(crate) executed_contracts: ExecutedContracts,
    // Cairo 1 contracts that are declared during the OS execution.
    pub(crate) declared_class_hash_to_component_hashes:
        HashMap<ClassHash, ContractClassComponentHashes>,
}

pub(crate) struct ExecutedContracts {
    pub(crate) contracts: HashMap<CompiledClassHash, CasmContractClass>,
    pub(crate) deprecated_contracts: HashMap<CompiledClassHash, DeprecatedContractClass>,
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
    let (default_initial_state_txs, execution_contracts, nonce_manager) =
        create_default_initial_state_txs_and_contracts();
    let fee_token_deploy_idx = default_initial_state_txs.len() - 1;

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
        execution_outputs[fee_token_deploy_idx]
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
    let mut commitment_storage = MapStorage::new();
    let mut borrowed_map_storage = BorrowedMapStorage { storage: &mut commitment_storage };
    let commitment_output =
        commit_initial_state_diff(committer_state_diff, &mut borrowed_map_storage).await;

    let initial_state = InitialState {
        updatable_state: final_state.state,
        commitment_storage,
        contracts_trie_root_hash: commitment_output.contracts_trie_root_hash,
        classes_trie_root_hash: commitment_output.classes_trie_root_hash,
    };

    (InitialStateData { initial_state, execution_contracts }, nonce_manager)
}

pub(crate) fn create_default_initial_state_txs_and_contracts()
-> (Vec<Transaction>, OsExecutionContracts, NonceManager) {
    // Declare account and ERC20 contracts.
    let account_contract =
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let account_sierra = account_contract.get_sierra();
    let account_contract = account_contract.get_class();
    let ContractClass::V1((account_casm, _sierra_version)) = account_contract else {
        panic!("Expected a V1 contract class, but got: {:?}", account_contract);
    };
    let account_declare_tx = create_cairo1_bootstrap_declare_tx(&account_sierra, &account_casm);
    let erc20_contract = FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let erc20_sierra = erc20_contract.get_sierra();
    let erc20_class = erc20_contract.get_class();
    let ContractClass::V1((erc20_casm, _sierra_version)) = erc20_class else {
        panic!("Expected a V1 contract class, but got: {:?}", erc20_class);
    };
    let erc20_declare_tx = create_cairo1_bootstrap_declare_tx(&erc20_sierra, &erc20_casm);

    let mut txs = vec![
        Transaction::new_for_sequencing(StarknetAPITransaction::Account(account_declare_tx)),
        Transaction::new_for_sequencing(StarknetAPITransaction::Account(erc20_declare_tx)),
    ];
    let mut nonce_manager = NonceManager::default();

    // Deploy an account.
    let account_contract_class_hash = account_sierra.calculate_class_hash();
    let deploy_account_tx_args = deploy_account_tx_args! {
        constructor_calldata: Calldata(vec![].into()),
        class_hash: account_contract_class_hash,
    };
    let nonce = nonce_manager.next(*FUNDED_ACCOUNT_ADDRESS);
    let deploy_tx = deploy_account_tx(deploy_account_tx_args, nonce);
    let deploy_tx = DeployAccountTransaction::create(deploy_tx, &CHAIN_ID_FOR_TESTS).unwrap();
    let account_contract_address = deploy_tx.contract_address;
    // Sanity check to verify the FUNDED_ACCOUNT_ADDRESS constant.
    assert_eq!(account_contract_address, *FUNDED_ACCOUNT_ADDRESS);
    let deploy_tx = Transaction::new_for_sequencing(StarknetAPITransaction::Account(
        AccountTransaction::DeployAccount(deploy_tx),
    ));
    txs.push(deploy_tx);

    // Deploy token contract using the deploy syscall.
    let erc20_class_hash = erc20_sierra.calculate_class_hash();
    let constructor_calldata = [
        9.into(),                                   // constructor length
        Felt::from_bytes_be_slice(b"STARK"),        // token name
        Felt::from_bytes_be_slice(b"STARK_SYMBOL"), // token symbol
        10.into(),                                  // token decimals
        INITIAL_TOKEN_SUPPLY.into(),                // initial supply lsb
        0.into(),                                   // initial supply msb
        *account_contract_address.0.key(),          // recipient address
        *account_contract_address.0.key(),          // permitted minter
        *account_contract_address.0.key(),          // provisional_governance_admin
        10.into(),                                  // upgrade delay
    ];
    let contract_address_salt = Felt::ONE;
    let calldata: Vec<_> = [erc20_class_hash.0, contract_address_salt]
        .into_iter()
        .chain(constructor_calldata)
        .collect();
    let deploy_contract_calldata = create_calldata(
        account_contract_address,
        DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME,
        &calldata,
    );
    let nonce = nonce_manager.next(account_contract_address);
    let invoke_tx_args = invoke_tx_args! {
        sender_address: account_contract_address,
        nonce,
        calldata: deploy_contract_calldata,

    };
    let deploy_contract_tx = invoke_tx(invoke_tx_args);
    let deploy_contract_tx =
        InvokeTransaction::create(deploy_contract_tx, &CHAIN_ID_FOR_TESTS).unwrap();
    let deploy_contract_tx = Transaction::new_for_sequencing(StarknetAPITransaction::Account(
        AccountTransaction::Invoke(deploy_contract_tx),
    ));
    txs.push(deploy_contract_tx);
    let erc20_compiled_class_hash = CompiledClassHash(erc20_casm.compiled_class_hash());
    let account_contract_compiled_class_hash =
        CompiledClassHash(account_casm.compiled_class_hash());

    let contracts = HashMap::from([
        (erc20_compiled_class_hash, erc20_casm),
        (account_contract_compiled_class_hash, account_casm),
    ]);
    let executed_contracts = ExecutedContracts { contracts, deprecated_contracts: HashMap::new() };
    let declared_class_hash_to_component_hashes = HashMap::from([
        (account_contract_class_hash, account_sierra.get_component_hashes(*API_VERSION)),
        (erc20_class_hash, erc20_sierra.get_component_hashes(*API_VERSION)),
    ]);
    (
        txs,
        OsExecutionContracts { executed_contracts, declared_class_hash_to_component_hashes },
        nonce_manager,
    )
}

pub(crate) async fn commit_initial_state_diff(
    committer_state_diff: StateDiff,
    borrowed_map_storage: &mut BorrowedMapStorage<'_>,
) -> CommitmentOutput {
    let classes_trie_root = HashOutput::ROOT_OF_EMPTY_TREE;
    let contract_trie_root = HashOutput::ROOT_OF_EMPTY_TREE;
    commit_state_diff(
        borrowed_map_storage,
        contract_trie_root,
        classes_trie_root,
        committer_state_diff,
    )
    .await
}
