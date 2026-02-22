use std::sync::Arc;

use apollo_storage::state::StateStorageReader;
use apollo_storage::{bootstrap_contracts, StorageReader};
use serde::{Deserialize, Serialize};
use starknet_api::abi::abi_utils::{get_storage_var_address, selector_from_name};
use starknet_api::block::GasPrice;
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
use tracing::info;

/// The felt representation of the string 'BOOTSTRAP', used as the sender address for bootstrap
/// declare transactions.
pub(crate) const BOOTSTRAP_SENDER_ADDRESS: u128 = 0x424f4f545354524150;

/// High gas amount sufficient to avoid out-of-gas errors during bootstrap.
const BOOTSTRAP_GAS_AMOUNT: u64 = 10_000_000_000;

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

impl BootstrapParams {
    fn new() -> Self {
        let account_contract_class = bootstrap_contracts::bootstrap_account_sierra();
        let account_class_hash = bootstrap_contracts::bootstrap_account_class_hash();
        let account_compiled_class_hash =
            bootstrap_contracts::bootstrap_account_compiled_class_hash();

        let erc20_contract_class = bootstrap_contracts::bootstrap_erc20_sierra();
        let erc20_class_hash = bootstrap_contracts::bootstrap_erc20_class_hash();
        let erc20_compiled_class_hash = bootstrap_contracts::bootstrap_erc20_compiled_class_hash();

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

        Self {
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

    /// Returns the account contract class hash (for tests).
    pub(crate) fn account_class_hash(&self) -> ClassHash {
        self.params.as_ref().map(|p| p.account_class_hash).unwrap_or_default()
    }

    /// Returns the account compiled class hash (for tests).
    pub(crate) fn account_compiled_class_hash(&self) -> CompiledClassHash {
        self.params.as_ref().map(|p| p.account_compiled_class_hash).unwrap_or_default()
    }

    /// Returns the ERC20 contract class hash (for tests).
    pub(crate) fn erc20_class_hash(&self) -> ClassHash {
        self.params.as_ref().map(|p| p.erc20_class_hash).unwrap_or_default()
    }

    /// Returns the ERC20 compiled class hash (for tests).
    pub(crate) fn erc20_compiled_class_hash(&self) -> CompiledClassHash {
        self.params.as_ref().map(|p| p.erc20_compiled_class_hash).unwrap_or_default()
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
        let params = self.params.as_ref().expect(
            "BootstrapStateMachine invariant: params is Some when bootstrap_enabled is true",
        );
        info!("Bootstrap: declaring account and ERC20 contract classes");
        let resource_bounds = Self::no_fee_resource_bounds();
        let bootstrap_address = ContractAddress::from(BOOTSTRAP_SENDER_ADDRESS);

        let account_declare =
            RpcTransaction::Declare(RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
                sender_address: bootstrap_address,
                compiled_class_hash: params.account_compiled_class_hash,
                signature: TransactionSignature::default(),
                nonce: Nonce::default(),
                contract_class: params.account_contract_class.clone(),
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
                compiled_class_hash: params.erc20_compiled_class_hash,
                signature: TransactionSignature::default(),
                nonce: Nonce::default(),
                contract_class: params.erc20_contract_class.clone(),
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
        let params = self.params.as_ref().expect(
            "BootstrapStateMachine invariant: params is Some when bootstrap_enabled is true",
        );
        info!("Bootstrap: deploying funded account");
        let resource_bounds = Self::no_fee_resource_bounds();

        let deploy_account = RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(
            RpcDeployAccountTransactionV3 {
                signature: TransactionSignature::default(),
                nonce: Nonce::default(),
                class_hash: params.account_class_hash,
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
        let params = self.params.as_ref().expect(
            "BootstrapStateMachine invariant: params is Some when bootstrap_enabled is true",
        );
        info!("Bootstrap: deploying STRK ERC20 fee token");
        let resource_bounds = Self::no_fee_resource_bounds();

        // The account nonce after deploy_account is 1.
        let nonce = Nonce(StarkHash::from(STRK_DEPLOY_NONCE));
        let salt = ContractAddressSalt(nonce.0);

        let deploy_contract_selector =
            selector_from_name(DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME);

        // The deploy_contract entry point expects:
        //   [class_hash, salt, ctor_calldata_len, ...ctor_calldata]
        // The erc20_testing constructor takes no arguments, so ctor_calldata is empty.
        let inner_calldata = vec![params.erc20_class_hash.0, salt.0, StarkHash::from(0_u128)];

        // The account's __execute__ expects calldata in the format:
        //   [contract_address, entry_point_selector, calldata_len, ...calldata]
        let execute_calldata: Vec<StarkHash> = [
            *params.account_address.0.key(),
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
                sender_address: params.account_address,
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
        let params = self.params.as_ref().expect(
            "BootstrapStateMachine invariant: params is Some when bootstrap_enabled is true",
        );
        info!("Bootstrap: funding account via ERC20 initial_funding");
        let resource_bounds = Self::no_fee_resource_bounds();
        let nonce = Nonce(StarkHash::from(POST_STRK_DEPLOY_NONCE));

        let initial_funding_selector = selector_from_name("initial_funding");

        // The initial_funding entry point expects a single argument: recipient address.
        let inner_calldata = vec![*params.account_address.0.key()];

        // The account's __execute__ expects calldata in the format:
        //   [contract_address, entry_point_selector, calldata_len, ...calldata]
        let execute_calldata: Vec<StarkHash> = [
            *params.strk_address.0.key(),
            initial_funding_selector.0,
            StarkHash::from(
                u128::try_from(inner_calldata.len()).expect("calldata length overflow"),
            ),
        ]
        .into_iter()
        .chain(inner_calldata)
        .collect();

        let fund_tx = RpcTransaction::Invoke(RpcInvokeTransaction::V3(RpcInvokeTransactionV3 {
            sender_address: params.account_address,
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
