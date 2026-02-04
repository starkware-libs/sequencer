//! Bootstrap transaction generator for starting a node with empty storage.
//!
//! This module generates hardcoded bootstrap transactions that initialize the system state:
//! - Declare account and ERC20 contracts (bootstrap mode, no fees)
//! - Deploy a funded account
//! - Deploy ETH and STRK fee tokens
//! - Fund the account with initial balances
//!
//! All addresses are deterministically calculated, so they can be known before deployment.

use std::sync::LazyLock;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::core::{calculate_contract_address, ClassHash, ContractAddress, Nonce};
use starknet_api::executable_transaction::DeclareTransaction;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::declare::rpc_declare_tx;
use starknet_api::test_utils::deploy_account::rpc_deploy_account_tx;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::constants::DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{
    Calldata,
    ContractAddressSalt,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::{declare_tx_args, deploy_account_tx_args, invoke_tx_args};
use starknet_types_core::felt::Felt;

/// Initial token supply for fee tokens (10^34).
const INITIAL_TOKEN_SUPPLY: u128 = 10_000_000_000_000_000_000_000_000_000_000_000;
const STRK_TOKEN_NAME: &[u8] = b"StarkNet Token";
const STRK_TOKEN_SYMBOL: &[u8] = b"STRK";
const ETH_TOKEN_NAME: &[u8] = b"Ethereum Token";
const ETH_TOKEN_SYMBOL: &[u8] = b"ETH";
const TOKEN_DECIMALS: u8 = 18;
/// Upgrade delay for fee tokens (in seconds).
const TOKEN_UPGRADE_DELAY: u64 = 10;

/// The account contract used for bootstrap.
pub fn bootstrap_account_contract() -> FeatureContract {
    FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm))
}

/// The ERC20 contract used for fee tokens.
pub fn bootstrap_erc20_contract() -> FeatureContract {
    FeatureContract::ERC20(CairoVersion::Cairo1(RunnableCairo1::Casm))
}

/// The deterministically calculated address of the funded account.
/// This is derived from the account class hash and default salt.
pub static BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS: LazyLock<ContractAddress> = LazyLock::new(|| {
    let class_hash = bootstrap_account_contract().get_sierra().calculate_class_hash();
    calculate_contract_address(
        ContractAddressSalt::default(),
        class_hash,
        &Calldata::default(),
        ContractAddress::default(), // deployer_address is zero for deploy account
    )
    .expect("Failed to calculate funded account address")
});

/// The deterministically calculated address of the STRK fee token.
/// This is derived from the ERC20 class hash, deployer (funded account), salt, and constructor
/// args.
pub static BOOTSTRAP_STRK_FEE_TOKEN_ADDRESS: LazyLock<ContractAddress> = LazyLock::new(|| {
    let class_hash = bootstrap_erc20_contract().get_sierra().calculate_class_hash();
    let constructor_calldata = strk_constructor_calldata();
    // Salt is based on the nonce (which is 1 after deploy account)
    let salt = ContractAddressSalt(Felt::ONE);
    calculate_contract_address(salt, class_hash, &constructor_calldata, *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS)
        .expect("Failed to calculate STRK fee token address")
});

/// The deterministically calculated address of the ETH fee token.
/// This is derived from the ERC20 class hash, deployer (funded account), salt, and constructor
/// args.
pub static BOOTSTRAP_ETH_FEE_TOKEN_ADDRESS: LazyLock<ContractAddress> = LazyLock::new(|| {
    let class_hash = bootstrap_erc20_contract().get_sierra().calculate_class_hash();
    let constructor_calldata = eth_constructor_calldata();
    // Salt is based on the nonce (which is 2 after deploying STRK token)
    let salt = ContractAddressSalt(Felt::TWO);
    calculate_contract_address(salt, class_hash, &constructor_calldata, *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS)
        .expect("Failed to calculate ETH fee token address")
});

/// Constructor calldata for STRK fee token.
fn strk_constructor_calldata() -> Calldata {
    Calldata(
        vec![
            Felt::from_bytes_be_slice(STRK_TOKEN_NAME),
            Felt::from_bytes_be_slice(STRK_TOKEN_SYMBOL),
            TOKEN_DECIMALS.into(),
            INITIAL_TOKEN_SUPPLY.into(), // initial supply lsb
            Felt::ZERO,                   // initial supply msb
            *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS.0.key(), // recipient address
            *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS.0.key(), // permitted minter
            *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS.0.key(), // provisional_governance_admin
            TOKEN_UPGRADE_DELAY.into(),
        ]
        .into(),
    )
}

/// Constructor calldata for ETH fee token.
fn eth_constructor_calldata() -> Calldata {
    Calldata(
        vec![
            Felt::from_bytes_be_slice(ETH_TOKEN_NAME),
            Felt::from_bytes_be_slice(ETH_TOKEN_SYMBOL),
            TOKEN_DECIMALS.into(),
            INITIAL_TOKEN_SUPPLY.into(), // initial supply lsb
            Felt::ZERO,                   // initial supply msb
            *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS.0.key(), // recipient address
            *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS.0.key(), // permitted minter
            *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS.0.key(), // provisional_governance_admin
            TOKEN_UPGRADE_DELAY.into(),
        ]
        .into(),
    )
}

/// Generate a bootstrap declare transaction (no fees, from bootstrap address).
fn generate_bootstrap_declare_tx(contract: FeatureContract) -> RpcTransaction {
    let sierra = contract.get_sierra();
    let class_hash = sierra.calculate_class_hash();
    let compiled_class_hash = contract.get_compiled_class_hash(
        &starknet_api::contract_class::compiled_class_hash::HashVersion::V2,
    );

    let declare_args = declare_tx_args!(
        signature: TransactionSignature::default(),
        sender_address: DeclareTransaction::bootstrap_address(),
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
        nonce: Nonce(Felt::ZERO),
        class_hash: class_hash,
        compiled_class_hash: compiled_class_hash,
    );
    rpc_declare_tx(declare_args, sierra)
}

/// Generate the deploy account transaction for the funded account.
fn generate_deploy_account_tx() -> RpcTransaction {
    let class_hash = bootstrap_account_contract().get_sierra().calculate_class_hash();
    let deploy_account_args = deploy_account_tx_args!(
        class_hash: class_hash,
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
        contract_address_salt: ContractAddressSalt::default(),
    );
    rpc_deploy_account_tx(deploy_account_args)
}

/// Generate an invoke transaction to deploy a fee token.
fn generate_deploy_fee_token_tx(
    class_hash: ClassHash,
    constructor_calldata: Calldata,
    nonce: Nonce,
    salt: ContractAddressSalt,
) -> RpcTransaction {
    let calldata_vec: Vec<Felt> =
        [class_hash.0, salt.0, (constructor_calldata.0.len() as u64).into()]
            .iter()
            .chain(constructor_calldata.0.iter())
            .cloned()
            .collect();

    let deploy_contract_calldata = create_calldata(
        *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS,
        DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME,
        &calldata_vec,
    );

    let invoke_args = invoke_tx_args!(
        sender_address: *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS,
        nonce: nonce,
        calldata: deploy_contract_calldata,
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
    );
    rpc_invoke_tx(invoke_args)
}

/// Generates all bootstrap transactions required to initialize the system.
///
/// Returns a vector of RPC transactions in the order they should be executed:
/// 1. Declare account contract (bootstrap mode)
/// 2. Declare ERC20 contract (bootstrap mode)
/// 3. Deploy funded account
/// 4. Deploy STRK fee token
/// 5. Deploy ETH fee token
///
/// Note: The initial token supply is minted to the funded account during ERC20 deployment
/// via the constructor, so no separate mint transaction is needed.
pub fn generate_bootstrap_transactions() -> Vec<RpcTransaction> {
    let account_contract = bootstrap_account_contract();
    let erc20_contract = bootstrap_erc20_contract();
    let erc20_class_hash = erc20_contract.get_sierra().calculate_class_hash();

    vec![
        // 1. Declare account contract (bootstrap mode - no fees)
        generate_bootstrap_declare_tx(account_contract),
        // 2. Declare ERC20 contract (bootstrap mode - no fees)
        generate_bootstrap_declare_tx(erc20_contract),
        // 3. Deploy funded account
        generate_deploy_account_tx(),
        // 4. Deploy STRK fee token (nonce 1 - first tx from funded account)
        generate_deploy_fee_token_tx(
            erc20_class_hash,
            strk_constructor_calldata(),
            Nonce(Felt::ONE),
            ContractAddressSalt(Felt::ONE),
        ),
        // 5. Deploy ETH fee token (nonce 2)
        generate_deploy_fee_token_tx(
            erc20_class_hash,
            eth_constructor_calldata(),
            Nonce(Felt::TWO),
            ContractAddressSalt(Felt::TWO),
        ),
    ]
}

/// Returns the bootstrap configuration values that should be used in node config.
/// This includes the deterministic addresses for fee tokens and the funded account.
#[derive(Debug, Clone)]
pub struct BootstrapAddresses {
    pub funded_account_address: ContractAddress,
    pub eth_fee_token_address: ContractAddress,
    pub strk_fee_token_address: ContractAddress,
}

impl BootstrapAddresses {
    /// Get the bootstrap addresses (deterministically calculated).
    pub fn get() -> Self {
        Self {
            funded_account_address: *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS,
            eth_fee_token_address: *BOOTSTRAP_ETH_FEE_TOKEN_ADDRESS,
            strk_fee_token_address: *BOOTSTRAP_STRK_FEE_TOKEN_ADDRESS,
        }
    }
}

// =============================================================================
// Bootstrap State and Execution (Stub Implementation)
// =============================================================================

/// The current state of the bootstrap process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BootstrapState {
    /// Bootstrap mode is not enabled or storage is not empty.
    #[default]
    Disabled,
    /// Bootstrap mode is enabled and storage is empty - waiting to start.
    Pending,
    /// Bootstrap transactions are being executed.
    InProgress,
    /// Bootstrap has completed successfully.
    Completed,
}

/// Manages the bootstrap process lifecycle.
///
/// This is a stub implementation that tracks state but does not yet inject
/// transactions into the batcher. The actual implementation will need to:
/// 1. Detect empty storage at startup
/// 2. Inject bootstrap transactions into batcher
/// 3. Monitor for completion (ERC20 balance checks)
/// 4. Transition to normal operation
#[derive(Debug)]
pub struct BootstrapManager {
    state: BootstrapState,
    addresses: BootstrapAddresses,
}

impl BootstrapManager {
    /// Create a new bootstrap manager.
    pub fn new() -> Self {
        Self { state: BootstrapState::default(), addresses: BootstrapAddresses::get() }
    }

    /// Get the current bootstrap state.
    pub fn state(&self) -> BootstrapState {
        self.state
    }

    /// Get the bootstrap addresses.
    pub fn addresses(&self) -> &BootstrapAddresses {
        &self.addresses
    }

    /// Check if storage is empty and bootstrap mode should be enabled.
    ///
    /// STUB: This currently always returns false.
    /// TODO: Implement by checking if header_marker == 0 in storage.
    pub fn should_enable_bootstrap(&self, _enable_bootstrap_mode: bool) -> bool {
        // TODO: Check if storage is empty (header_marker == 0)
        // For now, return false (disabled)
        false
    }

    /// Transition to pending state if bootstrap should be enabled.
    ///
    /// STUB: Does nothing currently.
    pub fn maybe_enter_pending(&mut self, enable_bootstrap_mode: bool) {
        if enable_bootstrap_mode && self.should_enable_bootstrap(enable_bootstrap_mode) {
            self.state = BootstrapState::Pending;
        }
    }

    /// Start the bootstrap process by injecting transactions.
    ///
    /// STUB: This currently just transitions state.
    /// TODO: Inject bootstrap transactions into batcher with validation disabled.
    pub fn start_bootstrap(&mut self) {
        if self.state == BootstrapState::Pending {
            self.state = BootstrapState::InProgress;
            // TODO: Actually inject transactions into batcher
            // let txs = generate_bootstrap_transactions();
            // batcher.inject_bootstrap_transactions(txs);
        }
    }

    /// Check if bootstrap is complete by verifying ERC20 balances.
    ///
    /// STUB: This currently always returns false.
    /// TODO: Implement by checking ERC20 balances in storage.
    pub fn check_completion(&mut self, _required_balance: u128) -> bool {
        if self.state != BootstrapState::InProgress {
            return false;
        }

        // TODO: Check ERC20 balances in storage:
        // let eth_balance = storage.get_storage_at(
        //     state_number,
        //     &self.addresses.eth_fee_token_address,
        //     &get_fee_token_var_address(self.addresses.funded_account_address)
        // );
        // let strk_balance = storage.get_storage_at(
        //     state_number,
        //     &self.addresses.strk_fee_token_address,
        //     &get_fee_token_var_address(self.addresses.funded_account_address)
        // );
        // if eth_balance >= required_balance && strk_balance >= required_balance {
        //     self.state = BootstrapState::Completed;
        //     return true;
        // }

        false
    }

    /// Check if bootstrap is in progress.
    pub fn is_in_progress(&self) -> bool {
        self.state == BootstrapState::InProgress
    }

    /// Check if bootstrap has completed.
    pub fn is_completed(&self) -> bool {
        self.state == BootstrapState::Completed
    }
}

impl Default for BootstrapManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bootstrap_addresses_are_deterministic() {
        // Access addresses multiple times to ensure they're consistently calculated
        let addresses1 = BootstrapAddresses::get();
        let addresses2 = BootstrapAddresses::get();

        assert_eq!(addresses1.funded_account_address, addresses2.funded_account_address);
        assert_eq!(addresses1.eth_fee_token_address, addresses2.eth_fee_token_address);
        assert_eq!(addresses1.strk_fee_token_address, addresses2.strk_fee_token_address);

        // Verify addresses are not zero/default
        assert_ne!(addresses1.funded_account_address, ContractAddress::default());
        assert_ne!(addresses1.eth_fee_token_address, ContractAddress::default());
        assert_ne!(addresses1.strk_fee_token_address, ContractAddress::default());
    }

    #[test]
    fn test_generate_bootstrap_transactions() {
        let txs = generate_bootstrap_transactions();
        assert_eq!(txs.len(), 5, "Should generate 5 bootstrap transactions");
    }

    #[test]
    fn test_bootstrap_manager_initial_state() {
        let manager = BootstrapManager::new();
        assert_eq!(manager.state(), BootstrapState::Disabled);
        assert!(!manager.is_in_progress());
        assert!(!manager.is_completed());
    }

    #[test]
    fn test_bootstrap_state_transitions() {
        let mut manager = BootstrapManager::new();

        // Initially disabled
        assert_eq!(manager.state(), BootstrapState::Disabled);

        // Should not enable because storage check stub returns false
        manager.maybe_enter_pending(true);
        assert_eq!(manager.state(), BootstrapState::Disabled);

        // Can't start bootstrap when not pending
        manager.start_bootstrap();
        assert_eq!(manager.state(), BootstrapState::Disabled);

        // Force state to Pending for testing
        manager.state = BootstrapState::Pending;
        manager.start_bootstrap();
        assert_eq!(manager.state(), BootstrapState::InProgress);
        assert!(manager.is_in_progress());

        // Check completion stub returns false
        assert!(!manager.check_completion(1000));
        assert_eq!(manager.state(), BootstrapState::InProgress);
    }
}
