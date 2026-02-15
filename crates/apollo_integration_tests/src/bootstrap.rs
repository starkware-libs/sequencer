//! Bootstrap transaction generator for starting a node with empty storage.
//!
//! This module generates hardcoded bootstrap transactions that initialize the system state:
//! - Declare account and ERC20 contracts (bootstrap mode, no fees)
//! - Deploy a funded account
//! - Deploy ETH and STRK fee tokens
//! - Fund the account with initial balances
//!
//! All addresses are deterministically calculated, so they can be known before deployment.
//!
//! # Usage
//!
//! ## Infrastructure Components
//!
//! 1. **Deterministic Addresses** - `BootstrapAddresses::get()`:
//!    - `funded_account_address`: The account that will receive initial token balances
//!    - `eth_fee_token_address`: ETH fee token contract address
//!    - `strk_fee_token_address`: STRK fee token contract address
//!
//! 2. **ChainInfo Configuration** - `BootstrapAddresses::create_chain_info_for_bootstrap()`:
//!    Creates a `ChainInfo` with fee tokens pointing to bootstrap addresses.
//!
//! 3. **Empty Storage** - `StorageTestSetup::new_empty_for_bootstrap()`: Creates storage without
//!    pre-populated accounts (in `state_reader.rs`).
//!
//! 4. **Empty Storage Detection** - `is_storage_empty()`: Returns true if storage has no committed
//!    blocks (header_marker == 0).
//!
//! 5. **Bootstrap Transactions** - `generate_bootstrap_transactions()`: Returns 5 RPC transactions
//!    that initialize the system.
//!
//! 6. **Internal Transactions** - `generate_bootstrap_internal_transactions()`: Returns
//!    transactions in `InternalConsensusTransaction` format for batcher injection.
//!
//! 7. **State Machine** - `BootstrapManager`: Manages bootstrap state transitions (Disabled ->
//!    Pending -> InProgress -> Completed).
//!
//! ## For Full End-to-End Bootstrap Testing
//!
//! Use `FlowTestSetup::new_for_bootstrap()` which:
//! 1. Creates empty storage with `StorageTestSetup::new_empty_for_bootstrap(chain_id)`
//! 2. Configures node with `BootstrapAddresses::create_chain_info_for_bootstrap()`
//! 3. Generates bootstrap transactions and passes them to the batcher
//! 4. Configures `BootstrapConfig` with deterministic addresses
//!
//! The batcher will:
//! 1. Start in Active bootstrap state (if storage is empty and txs provided)
//! 2. Include bootstrap transactions in initial block proposals
//! 3. Transition to Monitoring state after first block commit
//! 4. Check ERC20 balances after each block commit
//! 5. Transition to Completed state when balances are sufficient

use std::sync::LazyLock;

use apollo_storage::header::HeaderStorageReader;
use apollo_storage::state::StateStorageReader;
use apollo_storage::StorageReader;
use blockifier::context::{ChainInfo, FeeTokenAddresses};
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::abi::abi_utils::get_fee_token_var_address;
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{calculate_contract_address, ClassHash, ContractAddress, Nonce};
use starknet_api::executable_transaction::DeclareTransaction;
use starknet_api::rpc_transaction::{
    InternalRpcDeclareTransactionV3,
    InternalRpcDeployAccountTransaction,
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcTransaction,
};
use starknet_api::state::StateNumber;
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
use starknet_api::transaction::TransactionHash;
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
    calculate_contract_address(
        salt,
        class_hash,
        &constructor_calldata,
        *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS,
    )
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
    calculate_contract_address(
        salt,
        class_hash,
        &constructor_calldata,
        *BOOTSTRAP_FUNDED_ACCOUNT_ADDRESS,
    )
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
            Felt::ZERO,                  // initial supply msb
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
            Felt::ZERO,                  // initial supply msb
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
    let calldata_len: u64 =
        constructor_calldata.0.len().try_into().expect("calldata length overflow");
    let calldata_vec: Vec<Felt> = [class_hash.0, salt.0, calldata_len.into()]
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

    /// Create a ChainInfo configured with bootstrap fee token addresses.
    ///
    /// This is used when starting a node in bootstrap mode - the fee tokens
    /// will be deployed to these deterministic addresses during bootstrap.
    pub fn create_chain_info_for_bootstrap() -> ChainInfo {
        let addresses = Self::get();
        ChainInfo {
            chain_id: starknet_api::core::ChainId::IntegrationSepolia,
            fee_token_addresses: FeeTokenAddresses {
                eth_fee_token_address: addresses.eth_fee_token_address,
                strk_fee_token_address: addresses.strk_fee_token_address,
            },
            is_l3: false,
        }
    }
}

// =============================================================================
// Bootstrap State and Execution
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

/// Checks if the storage is empty (no blocks have been committed).
///
/// Returns true if header_marker is 0, meaning no blocks exist yet.
pub fn is_storage_empty(storage_reader: &StorageReader) -> bool {
    match storage_reader.begin_ro_txn() {
        Ok(txn) => match txn.get_header_marker() {
            Ok(marker) => marker == BlockNumber(0),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

/// Reads the ERC20 balance for a given account from storage.
///
/// Returns the balance as a u128, or 0 if the balance cannot be read.
pub fn get_erc20_balance(
    storage_reader: &StorageReader,
    token_address: ContractAddress,
    account_address: ContractAddress,
) -> u128 {
    let txn = match storage_reader.begin_ro_txn() {
        Ok(txn) => txn,
        Err(_) => return 0,
    };

    // Get the latest state number
    let header_marker = match txn.get_header_marker() {
        Ok(marker) => marker,
        Err(_) => return 0,
    };

    if header_marker == BlockNumber(0) {
        return 0; // No blocks committed yet
    }

    // State number is the block before header_marker
    let state_number =
        StateNumber::unchecked_right_after_block(BlockNumber(header_marker.0.saturating_sub(1)));

    // Get the storage key for the balance
    let balance_key = get_fee_token_var_address(account_address);

    // Read the balance from storage
    let state_reader = txn.get_state_reader();
    match state_reader {
        Ok(reader) => match reader.get_storage_at(state_number, &token_address, &balance_key) {
            Ok(balance) => {
                // Convert Felt to u128 (takes lower 128 bits)
                let bytes = balance.to_bytes_le();
                let mut arr = [0u8; 16];
                arr.copy_from_slice(&bytes[..16]);
                u128::from_le_bytes(arr)
            }
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

/// Convert an RPC transaction to an InternalConsensusTransaction.
///
/// This creates the internal format needed for batcher/consensus.
pub fn rpc_tx_to_internal_consensus_tx(
    rpc_tx: RpcTransaction,
    tx_hash: TransactionHash,
) -> InternalConsensusTransaction {
    let internal_tx = match rpc_tx {
        RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => {
            InternalRpcTransactionWithoutTxHash::Declare(InternalRpcDeclareTransactionV3 {
                signature: tx.signature,
                sender_address: tx.sender_address,
                resource_bounds: tx.resource_bounds,
                tip: tx.tip,
                nonce: tx.nonce,
                compiled_class_hash: tx.compiled_class_hash,
                paymaster_data: tx.paymaster_data,
                account_deployment_data: tx.account_deployment_data,
                nonce_data_availability_mode: tx.nonce_data_availability_mode,
                fee_data_availability_mode: tx.fee_data_availability_mode,
                class_hash: tx.contract_class.calculate_class_hash(),
            })
        }
        RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
            // Calculate the contract address for the deploy account transaction
            let contract_address = calculate_contract_address(
                tx.contract_address_salt,
                tx.class_hash,
                &tx.constructor_calldata,
                ContractAddress::default(),
            )
            .expect("Failed to calculate contract address for deploy account");

            InternalRpcTransactionWithoutTxHash::DeployAccount(
                InternalRpcDeployAccountTransaction {
                    tx: RpcDeployAccountTransaction::V3(tx),
                    contract_address,
                },
            )
        }
        RpcTransaction::Invoke(invoke_tx) => InternalRpcTransactionWithoutTxHash::Invoke(invoke_tx),
    };

    InternalConsensusTransaction::RpcTransaction(InternalRpcTransaction {
        tx: internal_tx,
        tx_hash,
    })
}

/// Generate bootstrap transactions as InternalConsensusTransaction format.
///
/// This is the format needed for injection into the batcher.
pub fn generate_bootstrap_internal_transactions() -> Vec<InternalConsensusTransaction> {
    generate_bootstrap_transactions()
        .into_iter()
        .enumerate()
        .map(|(i, rpc_tx)| {
            // Use a deterministic tx_hash based on index
            let idx: u64 = i.try_into().expect("transaction index overflow");
            let tx_hash = TransactionHash(Felt::from(idx));
            rpc_tx_to_internal_consensus_tx(rpc_tx, tx_hash)
        })
        .collect()
}

/// Manages the bootstrap process lifecycle.
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
    pub fn should_enable_bootstrap(
        &self,
        enable_bootstrap_mode: bool,
        storage_reader: &StorageReader,
    ) -> bool {
        enable_bootstrap_mode && is_storage_empty(storage_reader)
    }

    /// Transition to pending state if bootstrap should be enabled.
    pub fn maybe_enter_pending(
        &mut self,
        enable_bootstrap_mode: bool,
        storage_reader: &StorageReader,
    ) {
        if self.should_enable_bootstrap(enable_bootstrap_mode, storage_reader) {
            self.state = BootstrapState::Pending;
        }
    }

    /// Get the bootstrap transactions to inject.
    ///
    /// Returns the transactions in internal format, ready for batcher injection.
    pub fn get_bootstrap_transactions(&self) -> Vec<InternalConsensusTransaction> {
        generate_bootstrap_internal_transactions()
    }

    /// Start the bootstrap process.
    ///
    /// This transitions the state to InProgress. The caller is responsible for
    /// actually injecting the transactions (call get_bootstrap_transactions()).
    pub fn start_bootstrap(&mut self) {
        if self.state == BootstrapState::Pending {
            self.state = BootstrapState::InProgress;
        }
    }

    /// Check if bootstrap is complete by verifying ERC20 balances.
    ///
    /// Returns true if both ETH and STRK balances are >= required_balance.
    pub fn check_completion(
        &mut self,
        required_balance: u128,
        storage_reader: &StorageReader,
    ) -> bool {
        if self.state != BootstrapState::InProgress {
            return false;
        }

        let eth_balance = get_erc20_balance(
            storage_reader,
            self.addresses.eth_fee_token_address,
            self.addresses.funded_account_address,
        );

        let strk_balance = get_erc20_balance(
            storage_reader,
            self.addresses.strk_fee_token_address,
            self.addresses.funded_account_address,
        );

        if eth_balance >= required_balance && strk_balance >= required_balance {
            self.state = BootstrapState::Completed;
            return true;
        }

        false
    }

    /// Force set the state (for testing).
    #[cfg(test)]
    pub fn set_state(&mut self, state: BootstrapState) {
        self.state = state;
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
    fn test_generate_internal_transactions() {
        let txs = generate_bootstrap_internal_transactions();
        assert_eq!(txs.len(), 5, "Should generate 5 internal bootstrap transactions");
        // Verify all transactions are RPC transactions (not L1Handler)
        for tx in &txs {
            assert!(matches!(tx, InternalConsensusTransaction::RpcTransaction(_)));
        }
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

        // Can't start bootstrap when not pending
        manager.start_bootstrap();
        assert_eq!(manager.state(), BootstrapState::Disabled);

        // Force state to Pending for testing (using test helper)
        manager.set_state(BootstrapState::Pending);
        assert_eq!(manager.state(), BootstrapState::Pending);

        // Start bootstrap
        manager.start_bootstrap();
        assert_eq!(manager.state(), BootstrapState::InProgress);
        assert!(manager.is_in_progress());

        // Get bootstrap transactions
        let txs = manager.get_bootstrap_transactions();
        assert_eq!(txs.len(), 5);
    }
}
