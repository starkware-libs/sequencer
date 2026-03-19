//! Bootstrap phase detection and deterministic layout for embedded dummy account + fee token.
//!
//! ## `BootstrapLayout::EMBEDDED` vs `layout_if_bootstrap_enabled`
//!
//! - [`BootstrapLayout::EMBEDDED`] is fixed for this binary (class hashes and contract addresses
//!   from committed Sierra + deploy rules). Use it for transaction construction and tests even when
//!   [`BootstrapConfig::bootstrap_enabled`] is false.
//! - [`layout_if_bootstrap_enabled`] returns [`None`] when bootstrap is disabled in config; use it
//!   for HTTP/RPC or other surfaces that must not treat layout as operationally meaningful when the
//!   node has bootstrap turned off.

pub use apollo_batcher_config::BootstrapConfig;
use apollo_storage::state::StateStorageReader;
use apollo_storage::{bootstrap_contracts, StorageReader};
use serde::{Deserialize, Serialize};
use starknet_api::abi::abi_utils::get_storage_var_address;
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    ContractAddress,
    Nonce,
    PatriciaKey,
};
use starknet_api::hash::StarkHash;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::state::StateNumber;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};

#[cfg(test)]
#[path = "bootstrap_layout_test.rs"]
mod bootstrap_layout_test;

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

/// Deterministic class hashes and contract addresses for the embedded bootstrap contracts.
///
/// Values are compile-time constants; tests in `bootstrap_layout_test.rs` assert they match
/// embedded Sierra and [`calculate_contract_address`] with the same deploy rules as production.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapLayout {
    /// Class hash of the account contract (from embedded Sierra).
    pub account_class_hash: ClassHash,
    /// Class hash of the ERC20 contract (from embedded Sierra).
    pub erc20_class_hash: ClassHash,
    /// Deterministic address of the funded account (deploy account: salt 0, empty calldata).
    pub account_address: ContractAddress,
    /// Deterministic address of the STRK fee token (deploy from account, salt = nonce 1).
    pub strk_address: ContractAddress,
}

impl BootstrapLayout {
    /// Fixed layout for this binary. Kept in sync with embedded artifacts via unit tests.
    pub const EMBEDDED: Self = Self {
        account_class_hash: ClassHash(StarkHash::from_hex_unchecked(
            "0x023f6d63bd54a867e571beb1f98b5461f7f58b7647c01b2b4fb4b00c157bc709",
        )),
        erc20_class_hash: ClassHash(StarkHash::from_hex_unchecked(
            "0x0462b054af23d1f3b9da196a296ccdebfbabadee501bfb76e1c573cb93487abd",
        )),
        account_address: ContractAddress(PatriciaKey::from_hex_unchecked(
            "0x007da0b84832f1b32dc0b99e90af24ec05d3940690a388a60cadcb610ecf4903",
        )),
        strk_address: ContractAddress(PatriciaKey::from_hex_unchecked(
            "0x055379e04f508603662acdde5385fa769a06bca9e73a914e6e6b4f4fea6336ec",
        )),
    };
}

// TODO(victor): Use from call sites that need runtime-derived layout (e.g. bootstrap tx wiring,
// validation against chain) instead of duplicating this logic elsewhere.
#[allow(dead_code)]
pub(crate) fn derived_bootstrap_layout() -> BootstrapLayout {
    let account_class_hash = bootstrap_contracts::bootstrap_account_class_hash();
    let erc20_class_hash = bootstrap_contracts::bootstrap_erc20_class_hash();

    let account_address = calculate_contract_address(
        ContractAddressSalt::default(),
        account_class_hash,
        &Calldata::default(),
        ContractAddress::default(),
    )
    .expect("Failed to calculate account contract address");

    let strk_deploy_nonce = Nonce(StarkHash::from(PRE_FEE_TOKEN_SETUP_NONCE));
    let strk_constructor_calldata = Calldata(vec![*account_address.0.key()].into());
    let strk_address = calculate_contract_address(
        ContractAddressSalt(strk_deploy_nonce.0),
        erc20_class_hash,
        &strk_constructor_calldata,
        account_address,
    )
    .expect("Failed to calculate STRK fee token contract address");

    BootstrapLayout { account_class_hash, erc20_class_hash, account_address, strk_address }
}

/// When `config.bootstrap_enabled` is false, returns [`None`]. Otherwise returns
/// [`BootstrapLayout::EMBEDDED`].
pub fn layout_if_bootstrap_enabled(config: &BootstrapConfig) -> Option<&'static BootstrapLayout> {
    config.bootstrap_enabled.then_some(&BootstrapLayout::EMBEDDED)
}

/// Derives the current bootstrap phase from chain storage and config.
///
/// If `!config.bootstrap_enabled`, returns [`BootstrapState::NotInBootstrap`] without reading
/// storage. Otherwise uses [`BootstrapLayout::EMBEDDED`] and storage (declarations, deployments,
/// nonce, ERC20 `initialized`).
pub fn current_bootstrap_state(
    config: &BootstrapConfig,
    storage_reader: &StorageReader,
) -> BootstrapState {
    if !config.bootstrap_enabled {
        return BootstrapState::NotInBootstrap;
    }

    let layout = &BootstrapLayout::EMBEDDED;

    let txn = storage_reader.begin_ro_txn().expect("Failed to begin read-only transaction");
    let state_marker = txn.get_state_marker().expect("Failed to get state marker");
    let state_number = StateNumber::right_before_block(state_marker);
    let state_reader = txn.get_state_reader().expect("Failed to get state reader");

    let account_declared = state_reader
        .get_compiled_class_hash_at(state_number, &layout.account_class_hash)
        .expect("Failed to read account class hash")
        .is_some();
    let erc20_declared = state_reader
        .get_compiled_class_hash_at(state_number, &layout.erc20_class_hash)
        .expect("Failed to read ERC20 class hash")
        .is_some();
    if !account_declared || !erc20_declared {
        assert!(
            account_declared == erc20_declared,
            "Bootstrap fatal: partial class declaration (account_declared={account_declared}, \
             erc20_declared={erc20_declared}). A bootstrap declare transaction may have reverted."
        );
        return BootstrapState::DeclareContracts;
    }

    if state_reader
        .get_class_hash_at(state_number, &layout.account_address)
        .expect("Failed to read account deployment")
        .is_none()
    {
        return BootstrapState::DeployAccount;
    }

    let nonce = state_reader
        .get_nonce_at(state_number, &layout.account_address)
        .expect("Failed to read account nonce");
    if state_reader
        .get_class_hash_at(state_number, &layout.strk_address)
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

    let initialized_key = get_storage_var_address("initialized", &[]);
    let initialized = state_reader
        .get_storage_at(state_number, &layout.strk_address, &initialized_key)
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

/// Transactions to submit for `state` during bootstrap (stub until downstream wiring).
pub fn bootstrap_transactions_for_state(
    _config: &BootstrapConfig,
    _state: BootstrapState,
) -> Vec<RpcTransaction> {
    Vec::new()
}
