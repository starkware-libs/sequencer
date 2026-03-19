pub use apollo_batcher_config::config::BootstrapConfig;
use apollo_storage::state::StateStorageReader;
use apollo_storage::{bootstrap_contracts, StorageReader};
use serde::{Deserialize, Serialize};
use starknet_api::abi::abi_utils::get_storage_var_address;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::hash::StarkHash;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcTransaction,
};
use starknet_api::state::{SierraContractClass, StateNumber};
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use tracing::info;

/// The felt representation of the string 'BOOTSTRAP', used as the sender address for bootstrap
/// declare transactions.
const BOOTSTRAP_SENDER_ADDRESS: u128 = 0x424f4f545354524150;

#[cfg(test)]
#[path = "bootstrap_test.rs"]
mod bootstrap_test;

/// The state of the bootstrap process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootstrapState {
    /// Bootstrap is not active (either disabled or already complete).
    NotInBootstrap,
    /// First phase: declare the account and ERC20 contract classes.
    DeclareContracts,
    /// Second phase: deploy the funded account.
    DeployAccount,
    /// Third phase: deploy the STRK ERC20 fee token (constructor mints the full supply to the
    /// funded account).
    DeployFeeToken,
}

/// Account nonce after `deploy_account` (nonce 0 consumed); used as salt for STRK address and in
/// the "expected nonce 1" assert before fee-token setup.
pub(crate) const PRE_FEE_TOKEN_SETUP_NONCE: u128 = 1;
/// Account nonce after fee-token deploy (one invoke consumes nonce 1).
const POST_FEE_TOKEN_SETUP_NONCE: u128 = PRE_FEE_TOKEN_SETUP_NONCE + 1;

/// Class hash of the bootstrap account contract (not derived at runtime; update when the bundle
/// changes).
pub const BOOTSTRAP_ACCOUNT_CLASS_HASH: ClassHash = ClassHash(StarkHash::from_hex_unchecked(
    "0x023f6d63bd54a867e571beb1f98b5461f7f58b7647c01b2b4fb4b00c157bc709",
));
/// Class hash of the bootstrap ERC20 contract (not derived at runtime; update when the bundle
/// changes).
pub const BOOTSTRAP_ERC20_CLASS_HASH: ClassHash = ClassHash(StarkHash::from_hex_unchecked(
    "0x0462b054af23d1f3b9da196a296ccdebfbabadee501bfb76e1c573cb93487abd",
));
/// Deterministic address of the funded bootstrap account (deploy account: salt 0, empty calldata).
pub const BOOTSTRAP_ACCOUNT_ADDRESS: ContractAddress =
    ContractAddress(PatriciaKey::from_hex_unchecked(
        "0x007da0b84832f1b32dc0b99e90af24ec05d3940690a388a60cadcb610ecf4903",
    ));
/// Deterministic address of the STRK fee token (deploy from account, salt = nonce 1).
pub const BOOTSTRAP_STRK_ADDRESS: ContractAddress =
    ContractAddress(PatriciaKey::from_hex_unchecked(
        "0x055379e04f508603662acdde5385fa769a06bca9e73a914e6e6b4f4fea6336ec",
    ));

/// Derives the current bootstrap phase from chain storage and config.
///
/// If `!config.bootstrap_enabled`, returns [`BootstrapState::NotInBootstrap`] without reading
/// storage. Otherwise uses the `BOOTSTRAP_*` class-hash/address constants and storage
/// (declarations, deployments, nonce, ERC20 `initialized`).
pub fn current_bootstrap_state(
    config: &BootstrapConfig,
    storage_reader: &StorageReader,
) -> BootstrapState {
    if !config.bootstrap_enabled {
        return BootstrapState::NotInBootstrap;
    }

    let txn = storage_reader.begin_ro_txn().expect("Failed to begin read-only transaction");
    let state_marker = txn.get_state_marker().expect("Failed to get state marker");
    let state_number = StateNumber::right_before_block(state_marker);
    let state_reader = txn.get_state_reader().expect("Failed to get state reader");

    let account_declared = state_reader
        .get_compiled_class_hash_at(state_number, &BOOTSTRAP_ACCOUNT_CLASS_HASH)
        .expect("Failed to read account class hash")
        .is_some();
    let erc20_declared = state_reader
        .get_compiled_class_hash_at(state_number, &BOOTSTRAP_ERC20_CLASS_HASH)
        .expect("Failed to read ERC20 class hash")
        .is_some();
    if !account_declared || !erc20_declared {
        // Declares are separate txs; one class may be declared before the other.
        return BootstrapState::DeclareContracts;
    }

    if state_reader
        .get_class_hash_at(state_number, &BOOTSTRAP_ACCOUNT_ADDRESS)
        .expect("Failed to read account deployment")
        .is_none()
    {
        return BootstrapState::DeployAccount;
    }

    let nonce = state_reader
        .get_nonce_at(state_number, &BOOTSTRAP_ACCOUNT_ADDRESS)
        .expect("Failed to read account nonce");
    if state_reader
        .get_class_hash_at(state_number, &BOOTSTRAP_STRK_ADDRESS)
        .expect("Failed to read STRK deployment")
        .is_none()
    {
        assert!(
            nonce == Some(Nonce(StarkHash::from(PRE_FEE_TOKEN_SETUP_NONCE))),
            "Bootstrap fatal: ERC20 not deployed but account nonce is {nonce:?} (expected 1). The \
             deploy_fee_token transaction may have reverted."
        );
        return BootstrapState::DeployFeeToken;
    }

    // `initialized` is written in the erc20_testing constructor (`erc20_testing.cairo` storage).
    let initialized_key = get_storage_var_address("initialized", &[]);
    let initialized = state_reader
        .get_storage_at(state_number, &BOOTSTRAP_STRK_ADDRESS, &initialized_key)
        .expect("Failed to read ERC20 initialized flag");
    assert!(
        initialized != StarkHash::ZERO,
        "Bootstrap fatal: ERC20 deployed at expected address but `initialized` is false. Partial \
         or legacy bootstrap state is not supported."
    );
    assert!(
        nonce == Some(Nonce(StarkHash::from(POST_FEE_TOKEN_SETUP_NONCE))),
        "Bootstrap fatal: ERC20 ready but account nonce is {nonce:?} (expected 2)."
    );

    BootstrapState::NotInBootstrap
}

fn bootstrap_declare_v3_tx(
    sender_address: ContractAddress,
    resource_bounds: AllResourceBounds,
    compiled_class_hash: CompiledClassHash,
    contract_class: SierraContractClass,
) -> RpcTransaction {
    RpcTransaction::Declare(RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
        sender_address,
        compiled_class_hash,
        signature: TransactionSignature::default(),
        nonce: Nonce::default(),
        contract_class,
        resource_bounds,
        tip: Tip::default(),
        paymaster_data: PaymasterData::default(),
        account_deployment_data: AccountDeploymentData::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
    }))
}

fn bootstrap_declare_transactions() -> Vec<RpcTransaction> {
    let account_contract_class = bootstrap_contracts::bootstrap_account_sierra();
    let account_compiled_class_hash = bootstrap_contracts::bootstrap_account_compiled_class_hash();

    let erc20_contract_class = bootstrap_contracts::bootstrap_erc20_sierra();
    let erc20_compiled_class_hash = bootstrap_contracts::bootstrap_erc20_compiled_class_hash();

    info!("Bootstrap: declaring account and ERC20 contract classes");
    let ValidResourceBounds::AllResources(resource_bounds) =
        ValidResourceBounds::new_unlimited_gas_no_fee_enforcement()
    else {
        unreachable!("new_unlimited_gas_no_fee_enforcement returns AllResources");
    };
    let bootstrap_address = ContractAddress::from(BOOTSTRAP_SENDER_ADDRESS);

    vec![
        bootstrap_declare_v3_tx(
            bootstrap_address,
            resource_bounds,
            account_compiled_class_hash,
            account_contract_class,
        ),
        bootstrap_declare_v3_tx(
            bootstrap_address,
            resource_bounds,
            erc20_compiled_class_hash,
            erc20_contract_class,
        ),
    ]
}

fn bootstrap_deploy_account_transactions() -> Vec<RpcTransaction> {
    let layout = BootstrapLayout::EMBEDDED;
    info!("Bootstrap: deploying funded account");
    let resource_bounds = no_fee_resource_bounds();

    let deploy_account = RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(
        RpcDeployAccountTransactionV3 {
            signature: TransactionSignature::default(),
            nonce: Nonce::default(),
            class_hash: layout.account_class_hash,
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

/// Transactions to submit for `state` during bootstrap.
pub fn bootstrap_transactions_for_state(
    config: &BootstrapConfig,
    state: BootstrapState,
) -> Vec<RpcTransaction> {
    if !config.bootstrap_enabled {
        return Vec::new();
    }
    match state {
        BootstrapState::DeclareContracts => bootstrap_declare_transactions(),
        BootstrapState::DeployAccount => bootstrap_deploy_account_transactions(),
        BootstrapState::NotInBootstrap | BootstrapState::DeployFeeToken => Vec::new(),
    }
}
