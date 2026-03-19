use apollo_storage::state::StateStorageReader;
use apollo_storage::{bootstrap_contracts, StorageReader};
use serde::{Deserialize, Serialize};
use starknet_api::abi::abi_utils::get_storage_var_address;
use starknet_api::core::{calculate_contract_address, ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::state::StateNumber;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};

/// The state of the bootstrap process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootstrapState {
    /// Bootstrap is not active (either disabled or already complete).
    NotInBootstrap,
    /// First phase: declare the account and ERC20 contract classes.
    DeclareContracts,
    /// Second phase: deploy the funded account.
    DeployAccount,
    /// Third phase: deploy the STRK ERC20 fee token.
    DeployToken,
    /// Fourth phase: fund the account via the ERC20's `initial_funding`.
    FundAccount,
}

/// Account nonce after `deploy_account` (nonce 0 consumed); used as salt for STRK address and in
/// the "expected nonce 1" assert.
const STRK_DEPLOY_NONCE: u128 = 1;
/// Account nonce after STRK deploy (nonce 1 consumed); used in the "expected nonce 2" assert.
const POST_STRK_DEPLOY_NONCE: u128 = 2;

/// Configuration for the bootstrap process (e.g. whether it is enabled).
struct BootstrapConfig {
    bootstrap_enabled: bool,
}

/// Precomputed deterministic addresses and class hashes used during bootstrap.
struct BootstrapParams {
    /// Class hash of the account contract (computed from the Sierra class).
    account_class_hash: ClassHash,
    /// Class hash of the ERC20 contract (computed from the Sierra class).
    erc20_class_hash: ClassHash,
    /// Deterministic address of the funded account (computed from deploy account params).
    account_address: ContractAddress,
    /// Deterministic address of the STRK fee token contract.
    strk_address: ContractAddress,
}

impl BootstrapParams {
    fn new() -> Self {
        let account_class_hash = bootstrap_contracts::bootstrap_account_class_hash();
        let erc20_class_hash = bootstrap_contracts::bootstrap_erc20_class_hash();

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
        // Deployed via invoke from the account, with the account's nonce at STRK deploy time (1)
        // as salt (deploy_account consumes nonce 0).
        let strk_deploy_nonce = Nonce(StarkHash::from(STRK_DEPLOY_NONCE));
        let strk_address = calculate_contract_address(
            ContractAddressSalt(strk_deploy_nonce.0),
            erc20_class_hash,
            &Calldata::default(),
            account_address,
        )
        .expect("Failed to calculate STRK fee token contract address");

        Self { account_class_hash, erc20_class_hash, account_address, strk_address }
    }
}

/// Manages the bootstrap process for initializing a fresh node with required contracts.
///
/// The bootstrap state is derived from actual storage contents (class declarations,
/// contract deployments, nonce values, and storage variables), making the state machine
/// idempotent and crash-safe regardless of how many blocks have been produced.
pub struct BootstrapStateMachine {
    config: BootstrapConfig,
    params: Option<BootstrapParams>,
}

impl BootstrapStateMachine {
    /// Creates a new bootstrap state machine.
    ///
    /// If `bootstrap_enabled` is false, all calls to `current_state()` return
    /// `NotInBootstrap`. Contract info is loaded eagerly when enabled so that
    /// `current_state` + `transactions_for_state` are cheap.
    pub fn new(bootstrap_enabled: bool) -> Self {
        if !bootstrap_enabled {
            return Self { config: BootstrapConfig { bootstrap_enabled: false }, params: None };
        }

        Self {
            config: BootstrapConfig { bootstrap_enabled: true },
            params: Some(BootstrapParams::new()),
        }
    }

    /// Derives the current bootstrap state from actual storage contents.
    ///
    /// Checks class declarations, contract deployments, nonce values, and ERC20 storage
    /// to determine which bootstrap step should run next. Panics if a revert is detected
    /// (nonce consumed but expected side effect missing).
    pub fn current_state(&self, storage_reader: &StorageReader) -> BootstrapState {
        if !self.config.bootstrap_enabled {
            return BootstrapState::NotInBootstrap;
        }

        let params = self.params.as_ref().expect(
            "BootstrapStateMachine invariant: params is Some when bootstrap_enabled is true",
        );

        let txn = storage_reader.begin_ro_txn().expect("Failed to begin read-only transaction");
        let state_marker = txn.get_state_marker().expect("Failed to get state marker");
        let state_number = StateNumber::right_before_block(state_marker);
        let state_reader = txn.get_state_reader().expect("Failed to get state reader");

        let account_declared = state_reader
            .get_compiled_class_hash_at(state_number, &params.account_class_hash)
            .expect("Failed to read account class hash")
            .is_some();
        let erc20_declared = state_reader
            .get_compiled_class_hash_at(state_number, &params.erc20_class_hash)
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
            .get_class_hash_at(state_number, &params.account_address)
            .expect("Failed to read account deployment")
            .is_none()
        {
            return BootstrapState::DeployAccount;
        }

        let nonce = state_reader
            .get_nonce_at(state_number, &params.account_address)
            .expect("Failed to read account nonce");
        if state_reader
            .get_class_hash_at(state_number, &params.strk_address)
            .expect("Failed to read STRK deployment")
            .is_none()
        {
            assert!(
                nonce == Some(Nonce(StarkHash::from(STRK_DEPLOY_NONCE))),
                "Bootstrap fatal: ERC20 not deployed but account nonce is {nonce:?} (expected 1). \
                 The deploy_token transaction may have reverted."
            );
            return BootstrapState::DeployToken;
        }

        let initialized_key = get_storage_var_address("initialized", &[]);
        let initialized = state_reader
            .get_storage_at(state_number, &params.strk_address, &initialized_key)
            .expect("Failed to read ERC20 initialized flag");
        if initialized == StarkHash::ZERO {
            assert!(
                nonce == Some(Nonce(StarkHash::from(POST_STRK_DEPLOY_NONCE))),
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
    ///
    /// When bootstrap is disabled, returns the default address (only meaningful when enabled).
    pub fn account_address(&self) -> ContractAddress {
        self.params.as_ref().map(|p| p.account_address).unwrap_or_default()
    }

    /// Returns the deterministic STRK fee token address computed during initialization.
    ///
    /// When bootstrap is disabled, returns the default address (only meaningful when enabled).
    pub fn strk_address(&self) -> ContractAddress {
        self.params.as_ref().map(|p| p.strk_address).unwrap_or_default()
    }
}
