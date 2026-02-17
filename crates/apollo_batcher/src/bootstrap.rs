use std::sync::Arc;

use apollo_batcher_types::bootstrap_types::BootstrapState;
use apollo_storage::state::StateStorageReader;
use apollo_storage::StorageReader;
use blockifier_test_utils::cairo_versions::RunnableCairo1;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::abi::abi_utils::{get_storage_var_address, selector_from_name};
use starknet_api::block::GasPrice;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::hash::StarkHash;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::state::{SierraContractClass, StateNumber};
use starknet_api::transaction::constants::DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    PaymasterData,
    Proof,
    ProofFacts,
    ResourceBounds,
    Tip,
    TransactionSignature,
};

/// The felt representation of the string 'BOOTSTRAP', used as the sender address for bootstrap
/// declare transactions.
const BOOTSTRAP_SENDER_ADDRESS: u128 = 0x424f4f545354524150;

/// High gas amount sufficient to avoid out-of-gas errors during bootstrap.
const BOOTSTRAP_GAS_AMOUNT: u64 = 10_000_000_000;

/// Manages the bootstrap process for initializing a fresh node with required contracts.
///
/// The bootstrap state is derived from actual storage contents (class declarations,
/// contract deployments, nonce values, and storage variables), making the state machine
/// idempotent and crash-safe regardless of how many blocks have been produced.
pub struct BootstrapStateMachine {
    bootstrap_enabled: bool,
    /// Sierra contract class for the account contract.
    account_contract_class: SierraContractClass,
    /// Class hash of the account contract (computed from the Sierra class).
    account_class_hash: ClassHash,
    /// Compiled class hash of the account contract.
    account_compiled_class_hash: CompiledClassHash,
    /// Sierra contract class for the ERC20 fee token contract.
    erc20_contract_class: SierraContractClass,
    /// Class hash of the ERC20 contract (computed from the Sierra class).
    erc20_class_hash: ClassHash,
    /// Compiled class hash of the ERC20 contract.
    erc20_compiled_class_hash: CompiledClassHash,
    /// Deterministic address of the funded account (computed from deploy account params).
    account_address: ContractAddress,
    /// Deterministic address of the STRK fee token contract.
    strk_address: ContractAddress,
}

impl BootstrapStateMachine {
    /// Creates a new bootstrap state machine.
    ///
    /// If `bootstrap_enabled` is false, all calls to `current_state()` return
    /// `NotInBootstrap`. Contract info is loaded eagerly when enabled so that
    /// `current_state` + `transactions_for_state` are cheap.
    pub fn new(bootstrap_enabled: bool) -> Self {
        if !bootstrap_enabled {
            return Self::disabled();
        }

        let account_contract = FeatureContract::DummyAccount(RunnableCairo1::Casm);
        let erc20_contract = FeatureContract::ERC20Testing(RunnableCairo1::Casm);

        let account_contract_class = account_contract.get_sierra();
        let account_class_hash = account_contract_class.calculate_class_hash();
        let account_compiled_class_hash =
            account_contract.get_compiled_class_hash(&HashVersion::V2);

        let erc20_contract_class = erc20_contract.get_sierra();
        let erc20_class_hash = erc20_contract_class.calculate_class_hash();
        let erc20_compiled_class_hash = erc20_contract.get_compiled_class_hash(&HashVersion::V2);

        // Compute the account address deterministically.
        // Deploy account uses: salt=0, class_hash, empty calldata, deployer=0x0.
        let account_address = calculate_contract_address(
            ContractAddressSalt::default(),
            account_class_hash,
            &Calldata::default(),
            ContractAddress::default(),
        )
        .expect("Failed to calculate account contract address");

        // Compute the STRK fee token address deterministically.
        // Deployed via invoke from the account, with nonce as salt.
        // The nonce of the account at STRK deploy time is 1 (after deploy_account consumes
        // nonce 0).
        let strk_deploy_nonce = Nonce(StarkHash::from(1_u128));
        let strk_address = calculate_contract_address(
            ContractAddressSalt(strk_deploy_nonce.0),
            erc20_class_hash,
            &Calldata::default(),
            account_address,
        )
        .expect("Failed to calculate STRK fee token contract address");

        Self {
            bootstrap_enabled: true,
            account_contract_class,
            account_class_hash,
            account_compiled_class_hash,
            erc20_contract_class,
            erc20_class_hash,
            erc20_compiled_class_hash,
            account_address,
            strk_address,
        }
    }

    fn disabled() -> Self {
        Self {
            bootstrap_enabled: false,
            account_contract_class: SierraContractClass::default(),
            account_class_hash: ClassHash::default(),
            account_compiled_class_hash: CompiledClassHash::default(),
            erc20_contract_class: SierraContractClass::default(),
            erc20_class_hash: ClassHash::default(),
            erc20_compiled_class_hash: CompiledClassHash::default(),
            account_address: ContractAddress::default(),
            strk_address: ContractAddress::default(),
        }
    }

    /// Derives the current bootstrap state from actual storage contents.
    ///
    /// Checks class declarations, contract deployments, nonce values, and ERC20 storage
    /// to determine which bootstrap step should run next. Panics if a revert is detected
    /// (nonce consumed but expected side effect missing).
    pub fn current_state(&self, storage_reader: &StorageReader) -> BootstrapState {
        if !self.bootstrap_enabled {
            return BootstrapState::NotInBootstrap;
        }

        let txn = storage_reader.begin_ro_txn().expect("Failed to begin read-only transaction");
        let state_marker = txn.get_state_marker().expect("Failed to get state marker");
        let state_number = StateNumber::right_before_block(state_marker);
        let state_reader = txn.get_state_reader().expect("Failed to get state reader");

        let account_declared = state_reader
            .get_compiled_class_hash_at(state_number, &self.account_class_hash)
            .expect("Failed to read account class hash")
            .is_some();
        let erc20_declared = state_reader
            .get_compiled_class_hash_at(state_number, &self.erc20_class_hash)
            .expect("Failed to read ERC20 class hash")
            .is_some();
        if !account_declared || !erc20_declared {
            assert!(
                account_declared == erc20_declared,
                "Bootstrap fatal: partial class declaration (account_declared={account_declared}, \
                 erc20_declared={erc20_declared}). A bootstrap declare transaction may have \
                 reverted."
            );
            return BootstrapState::DeclareContracts;
        }

        if state_reader
            .get_class_hash_at(state_number, &self.account_address)
            .expect("Failed to read account deployment")
            .is_none()
        {
            return BootstrapState::DeployAccount;
        }

        let nonce = state_reader
            .get_nonce_at(state_number, &self.account_address)
            .expect("Failed to read account nonce");
        if state_reader
            .get_class_hash_at(state_number, &self.strk_address)
            .expect("Failed to read STRK deployment")
            .is_none()
        {
            assert!(
                nonce == Some(Nonce(StarkHash::from(1_u128))),
                "Bootstrap fatal: ERC20 not deployed but account nonce is {nonce:?} (expected 1). \
                 The deploy_token transaction may have reverted."
            );
            return BootstrapState::DeployToken;
        }

        let initialized_key = get_storage_var_address("initialized", &[]);
        let initialized = state_reader
            .get_storage_at(state_number, &self.strk_address, &initialized_key)
            .expect("Failed to read ERC20 initialized flag");
        if initialized == StarkHash::ZERO {
            assert!(
                nonce == Some(Nonce(StarkHash::from(2_u128))),
                "Bootstrap fatal: ERC20 not initialized but account nonce is {nonce:?} (expected \
                 2). The fund_account transaction may have reverted."
            );
            return BootstrapState::FundAccount;
        }

        BootstrapState::NotInBootstrap
    }

    /// Returns the transactions that should be submitted for the given bootstrap state.
    pub fn transactions_for_state(&self, state: BootstrapState) -> Vec<RpcTransaction> {
        match state {
            BootstrapState::DeclareContracts => self.declare_transactions(),
            BootstrapState::DeployAccount => self.deploy_account_transactions(),
            BootstrapState::DeployToken => self.deploy_token_transactions(),
            BootstrapState::FundAccount => self.fund_account_transactions(),
            BootstrapState::NotInBootstrap => Vec::new(),
        }
    }

    /// Returns the deterministic account address computed during initialization.
    pub fn account_address(&self) -> ContractAddress {
        self.account_address
    }

    /// Returns the deterministic STRK fee token address computed during initialization.
    pub fn strk_address(&self) -> ContractAddress {
        self.strk_address
    }

    fn no_fee_resource_bounds() -> AllResourceBounds {
        let default_resource =
            ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(1) };
        AllResourceBounds {
            l1_gas: default_resource,
            l2_gas: ResourceBounds {
                max_amount: GasAmount(BOOTSTRAP_GAS_AMOUNT),
                max_price_per_unit: GasPrice(0),
            },
            l1_data_gas: default_resource,
        }
    }

    /// Creates the declare transactions for the account and ERC20 contract classes.
    fn declare_transactions(&self) -> Vec<RpcTransaction> {
        let resource_bounds = Self::no_fee_resource_bounds();
        let bootstrap_address = ContractAddress::from(BOOTSTRAP_SENDER_ADDRESS);

        let account_declare =
            RpcTransaction::Declare(RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
                sender_address: bootstrap_address,
                compiled_class_hash: self.account_compiled_class_hash,
                signature: TransactionSignature::default(),
                nonce: Nonce::default(),
                contract_class: self.account_contract_class.clone(),
                resource_bounds,
                tip: Tip::default(),
                paymaster_data: PaymasterData::default(),
                account_deployment_data: AccountDeploymentData::default(),
                nonce_data_availability_mode: DataAvailabilityMode::L1,
                fee_data_availability_mode: DataAvailabilityMode::L1,
            }));

        let erc20_declare =
            RpcTransaction::Declare(RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
                sender_address: bootstrap_address,
                compiled_class_hash: self.erc20_compiled_class_hash,
                signature: TransactionSignature::default(),
                nonce: Nonce::default(),
                contract_class: self.erc20_contract_class.clone(),
                resource_bounds,
                tip: Tip::default(),
                paymaster_data: PaymasterData::default(),
                account_deployment_data: AccountDeploymentData::default(),
                nonce_data_availability_mode: DataAvailabilityMode::L1,
                fee_data_availability_mode: DataAvailabilityMode::L1,
            }));

        vec![account_declare, erc20_declare]
    }

    /// Creates the deploy account transaction for the funded account.
    fn deploy_account_transactions(&self) -> Vec<RpcTransaction> {
        let resource_bounds = Self::no_fee_resource_bounds();

        let deploy_account = RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(
            RpcDeployAccountTransactionV3 {
                signature: TransactionSignature::default(),
                nonce: Nonce::default(),
                class_hash: self.account_class_hash,
                contract_address_salt: ContractAddressSalt::default(),
                constructor_calldata: Calldata::default(),
                resource_bounds,
                tip: Tip::default(),
                paymaster_data: PaymasterData::default(),
                nonce_data_availability_mode: DataAvailabilityMode::L1,
                fee_data_availability_mode: DataAvailabilityMode::L1,
            },
        ));

        vec![deploy_account]
    }

    /// Creates the invoke transaction to deploy the STRK ERC20 fee token contract.
    ///
    /// The erc20_testing contract constructor takes no arguments.
    fn deploy_token_transactions(&self) -> Vec<RpcTransaction> {
        let resource_bounds = Self::no_fee_resource_bounds();

        // The account nonce after deploy_account is 1.
        let nonce = Nonce(StarkHash::from(1_u128));
        let salt = ContractAddressSalt(nonce.0);

        let deploy_contract_selector =
            selector_from_name(DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME);

        // The deploy_contract entry point expects:
        //   [class_hash, salt, ctor_calldata_len, ...ctor_calldata]
        // The erc20_testing constructor takes no arguments, so ctor_calldata is empty.
        let inner_calldata = vec![self.erc20_class_hash.0, salt.0, StarkHash::from(0_u128)];

        // The account's __execute__ expects calldata in the format:
        //   [contract_address, entry_point_selector, calldata_len, ...calldata]
        let execute_calldata: Vec<StarkHash> = [
            *self.account_address.0.key(),
            deploy_contract_selector.0,
            StarkHash::from(
                u128::try_from(inner_calldata.len()).expect("calldata length overflow"),
            ),
        ]
        .into_iter()
        .chain(inner_calldata)
        .collect();

        let strk_deploy =
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(RpcInvokeTransactionV3 {
                sender_address: self.account_address,
                calldata: Calldata(Arc::new(execute_calldata)),
                signature: TransactionSignature::default(),
                nonce,
                resource_bounds,
                tip: Tip::default(),
                paymaster_data: PaymasterData::default(),
                account_deployment_data: AccountDeploymentData::default(),
                nonce_data_availability_mode: DataAvailabilityMode::L1,
                fee_data_availability_mode: DataAvailabilityMode::L1,
                proof_facts: ProofFacts::default(),
                proof: Proof::default(),
            }));

        vec![strk_deploy]
    }

    /// Creates the invoke transaction to fund the account via the ERC20's `initial_funding`.
    ///
    /// Account nonce at this point is 2 (0 consumed by deploy_account, 1 by deploy_token).
    fn fund_account_transactions(&self) -> Vec<RpcTransaction> {
        let resource_bounds = Self::no_fee_resource_bounds();
        let nonce = Nonce(StarkHash::from(2_u128));

        let initial_funding_selector = selector_from_name("initial_funding");

        // The initial_funding entry point expects a single argument: recipient address.
        let inner_calldata = vec![*self.account_address.0.key()];

        // The account's __execute__ expects calldata in the format:
        //   [contract_address, entry_point_selector, calldata_len, ...calldata]
        let execute_calldata: Vec<StarkHash> = [
            *self.strk_address.0.key(),
            initial_funding_selector.0,
            StarkHash::from(
                u128::try_from(inner_calldata.len()).expect("calldata length overflow"),
            ),
        ]
        .into_iter()
        .chain(inner_calldata)
        .collect();

        let fund_tx = RpcTransaction::Invoke(RpcInvokeTransaction::V3(RpcInvokeTransactionV3 {
            sender_address: self.account_address,
            calldata: Calldata(Arc::new(execute_calldata)),
            signature: TransactionSignature::default(),
            nonce,
            resource_bounds,
            tip: Tip::default(),
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            proof_facts: ProofFacts::default(),
            proof: Proof::default(),
        }));

        vec![fund_tx]
    }
}

#[cfg(test)]
mod tests {
    use apollo_storage::state::StateStorageWriter;
    use apollo_storage::test_utils::get_test_storage;
    use indexmap::IndexMap;
    use starknet_api::block::BlockNumber;
    use starknet_api::state::ThinStateDiff;
    use starknet_types_core::felt::Felt;

    use super::*;

    fn create_test_sm_and_storage()
    -> (BootstrapStateMachine, StorageReader, apollo_storage::StorageWriter) {
        let sm = BootstrapStateMachine::new(true);
        let ((reader, writer), _temp_dir) = get_test_storage();
        std::mem::forget(_temp_dir);
        (sm, reader, writer)
    }

    fn declare_diff(sm: &BootstrapStateMachine) -> ThinStateDiff {
        ThinStateDiff {
            class_hash_to_compiled_class_hash: IndexMap::from([
                (sm.account_class_hash, sm.account_compiled_class_hash),
                (sm.erc20_class_hash, sm.erc20_compiled_class_hash),
            ]),
            ..Default::default()
        }
    }

    fn deploy_account_diff(sm: &BootstrapStateMachine) -> ThinStateDiff {
        ThinStateDiff {
            deployed_contracts: IndexMap::from([(sm.account_address, sm.account_class_hash)]),
            nonces: IndexMap::from([(sm.account_address, Nonce(StarkHash::from(1_u128)))]),
            ..Default::default()
        }
    }

    fn deploy_token_diff(sm: &BootstrapStateMachine) -> ThinStateDiff {
        ThinStateDiff {
            deployed_contracts: IndexMap::from([(sm.strk_address, sm.erc20_class_hash)]),
            nonces: IndexMap::from([(sm.account_address, Nonce(StarkHash::from(2_u128)))]),
            ..Default::default()
        }
    }

    fn fund_account_diff(sm: &BootstrapStateMachine) -> ThinStateDiff {
        let initialized_key = get_storage_var_address("initialized", &[]);
        ThinStateDiff {
            storage_diffs: IndexMap::from([(
                sm.strk_address,
                IndexMap::from([(initialized_key, Felt::ONE)]),
            )]),
            nonces: IndexMap::from([(sm.account_address, Nonce(StarkHash::from(3_u128)))]),
            ..Default::default()
        }
    }

    fn append_diff(writer: &mut apollo_storage::StorageWriter, block: u64, diff: ThinStateDiff) {
        writer
            .begin_rw_txn()
            .unwrap()
            .append_state_diff(BlockNumber(block), diff)
            .unwrap()
            .commit()
            .unwrap();
    }

    #[test]
    fn not_in_bootstrap_when_disabled() {
        let sm = BootstrapStateMachine::new(false);
        let ((reader, _writer), _temp_dir) = get_test_storage();
        assert_eq!(sm.current_state(&reader), BootstrapState::NotInBootstrap);
        assert!(sm.transactions_for_state(BootstrapState::NotInBootstrap).is_empty());
    }

    #[test]
    fn empty_storage_returns_declare_contracts() {
        let (sm, reader, _writer) = create_test_sm_and_storage();
        assert_eq!(sm.current_state(&reader), BootstrapState::DeclareContracts);
    }

    #[test]
    fn state_after_declare() {
        let (sm, reader, mut writer) = create_test_sm_and_storage();
        append_diff(&mut writer, 0, declare_diff(&sm));
        assert_eq!(sm.current_state(&reader), BootstrapState::DeployAccount);
    }

    #[test]
    fn state_after_deploy_account() {
        let (sm, reader, mut writer) = create_test_sm_and_storage();
        append_diff(&mut writer, 0, declare_diff(&sm));
        append_diff(&mut writer, 1, deploy_account_diff(&sm));
        assert_eq!(sm.current_state(&reader), BootstrapState::DeployToken);
    }

    #[test]
    fn state_after_deploy_token() {
        let (sm, reader, mut writer) = create_test_sm_and_storage();
        append_diff(&mut writer, 0, declare_diff(&sm));
        append_diff(&mut writer, 1, deploy_account_diff(&sm));
        append_diff(&mut writer, 2, deploy_token_diff(&sm));
        assert_eq!(sm.current_state(&reader), BootstrapState::FundAccount);
    }

    #[test]
    fn state_after_fund() {
        let (sm, reader, mut writer) = create_test_sm_and_storage();
        append_diff(&mut writer, 0, declare_diff(&sm));
        append_diff(&mut writer, 1, deploy_account_diff(&sm));
        append_diff(&mut writer, 2, deploy_token_diff(&sm));
        append_diff(&mut writer, 3, fund_account_diff(&sm));
        assert_eq!(sm.current_state(&reader), BootstrapState::NotInBootstrap);
    }

    #[test]
    #[should_panic(expected = "partial class declaration")]
    fn partial_declaration_panics() {
        let (sm, reader, mut writer) = create_test_sm_and_storage();
        let partial_diff = ThinStateDiff {
            class_hash_to_compiled_class_hash: IndexMap::from([(
                sm.account_class_hash,
                sm.account_compiled_class_hash,
            )]),
            ..Default::default()
        };
        append_diff(&mut writer, 0, partial_diff);
        sm.current_state(&reader);
    }

    #[test]
    #[should_panic(expected = "deploy_token transaction may have reverted")]
    fn deploy_token_revert_detected() {
        let (sm, reader, mut writer) = create_test_sm_and_storage();
        append_diff(&mut writer, 0, declare_diff(&sm));
        let bad_diff = ThinStateDiff {
            deployed_contracts: IndexMap::from([(sm.account_address, sm.account_class_hash)]),
            nonces: IndexMap::from([(sm.account_address, Nonce(StarkHash::from(2_u128)))]),
            ..Default::default()
        };
        append_diff(&mut writer, 1, bad_diff);
        sm.current_state(&reader);
    }

    #[test]
    #[should_panic(expected = "fund_account transaction may have reverted")]
    fn fund_account_revert_detected() {
        let (sm, reader, mut writer) = create_test_sm_and_storage();
        append_diff(&mut writer, 0, declare_diff(&sm));
        append_diff(&mut writer, 1, deploy_account_diff(&sm));
        let bad_diff = ThinStateDiff {
            deployed_contracts: IndexMap::from([(sm.strk_address, sm.erc20_class_hash)]),
            nonces: IndexMap::from([(sm.account_address, Nonce(StarkHash::from(3_u128)))]),
            ..Default::default()
        };
        append_diff(&mut writer, 2, bad_diff);
        sm.current_state(&reader);
    }

    #[test]
    fn declare_transactions_generated_correctly() {
        let sm = BootstrapStateMachine::new(true);
        let txs = sm.transactions_for_state(BootstrapState::DeclareContracts);

        assert_eq!(txs.len(), 2);
        assert!(matches!(txs[0], RpcTransaction::Declare(_)));
        assert!(matches!(txs[1], RpcTransaction::Declare(_)));

        let bootstrap_addr = ContractAddress::from(BOOTSTRAP_SENDER_ADDRESS);
        for tx in &txs {
            if let RpcTransaction::Declare(RpcDeclareTransaction::V3(ref declare)) = tx {
                assert_eq!(declare.sender_address, bootstrap_addr);
                assert_eq!(declare.nonce, Nonce::default());
            } else {
                panic!("Expected RpcDeclareTransaction::V3");
            }
        }
    }

    #[test]
    fn deploy_account_transaction_generated_correctly() {
        let sm = BootstrapStateMachine::new(true);
        let txs = sm.transactions_for_state(BootstrapState::DeployAccount);

        assert_eq!(txs.len(), 1);
        if let RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(ref deploy)) = txs[0] {
            assert_eq!(deploy.class_hash, sm.account_class_hash);
            assert_eq!(deploy.contract_address_salt, ContractAddressSalt::default());
            assert_eq!(deploy.nonce, Nonce::default());
        } else {
            panic!("Expected RpcDeployAccountTransaction::V3");
        }
    }

    #[test]
    fn deploy_token_transaction_generated_correctly() {
        let sm = BootstrapStateMachine::new(true);
        let txs = sm.transactions_for_state(BootstrapState::DeployToken);

        assert_eq!(txs.len(), 1);
        if let RpcTransaction::Invoke(RpcInvokeTransaction::V3(ref invoke)) = txs[0] {
            assert_eq!(invoke.sender_address, sm.account_address);
            assert_eq!(invoke.nonce, Nonce(StarkHash::from(1_u128)));
            assert!(!invoke.calldata.0.is_empty());
        } else {
            panic!("Expected RpcInvokeTransaction::V3");
        }
    }

    #[test]
    fn fund_account_transaction_generated_correctly() {
        let sm = BootstrapStateMachine::new(true);
        let txs = sm.transactions_for_state(BootstrapState::FundAccount);

        assert_eq!(txs.len(), 1);
        if let RpcTransaction::Invoke(RpcInvokeTransaction::V3(ref invoke)) = txs[0] {
            assert_eq!(invoke.sender_address, sm.account_address);
            assert_eq!(invoke.nonce, Nonce(StarkHash::from(2_u128)));
            assert!(!invoke.calldata.0.is_empty());
        } else {
            panic!("Expected RpcInvokeTransaction::V3");
        }
    }

    #[test]
    fn not_in_bootstrap_produces_no_transactions() {
        let sm = BootstrapStateMachine::new(true);
        assert!(sm.transactions_for_state(BootstrapState::NotInBootstrap).is_empty());
    }

    #[test]
    fn deterministic_addresses_are_consistent() {
        let sm1 = BootstrapStateMachine::new(true);
        let sm2 = BootstrapStateMachine::new(true);

        assert_eq!(sm1.account_address(), sm2.account_address());
        assert_eq!(sm1.strk_address(), sm2.strk_address());
        assert_ne!(sm1.account_address(), ContractAddress::default());
        assert_ne!(sm1.strk_address(), ContractAddress::default());
    }
}
