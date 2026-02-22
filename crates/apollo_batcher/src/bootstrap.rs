use apollo_batcher_types::bootstrap_types::BootstrapState;
use apollo_storage::state::StateStorageReader;
use apollo_storage::StorageReader;
use blockifier_test_utils::cairo_versions::RunnableCairo1;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::abi::abi_utils::get_storage_var_address;
use starknet_api::core::{calculate_contract_address, ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::state::StateNumber;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};

/// Manages the bootstrap process for initializing a fresh node with required contracts.
///
/// The bootstrap state is derived from actual storage contents (class declarations,
/// contract deployments, nonce values, and storage variables), making the state machine
/// idempotent and crash-safe regardless of how many blocks have been produced.
pub struct BootstrapStateMachine {
    bootstrap_enabled: bool,
    /// Class hash of the account contract (computed from the Sierra class).
    account_class_hash: ClassHash,
    /// Class hash of the ERC20 contract (computed from the Sierra class).
    erc20_class_hash: ClassHash,
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

        let account_class_hash = account_contract.get_sierra().calculate_class_hash();
        let erc20_class_hash = erc20_contract.get_sierra().calculate_class_hash();

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
            account_class_hash,
            erc20_class_hash,
            account_address,
            strk_address,
        }
    }

    fn disabled() -> Self {
        Self {
            bootstrap_enabled: false,
            account_class_hash: ClassHash::default(),
            erc20_class_hash: ClassHash::default(),
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
    pub fn transactions_for_state(&self, _state: BootstrapState) -> Vec<RpcTransaction> {
        Vec::new()
    }

    /// Returns the deterministic account address computed during initialization.
    pub fn account_address(&self) -> ContractAddress {
        self.account_address
    }

    /// Returns the deterministic STRK fee token address computed during initialization.
    pub fn strk_address(&self) -> ContractAddress {
        self.strk_address
    }
}
