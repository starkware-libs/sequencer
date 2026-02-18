use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::RpcTransaction;

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

/// Requests that can be made to the bootstrap HTTP server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BootstrapRequest {
    /// Query the current bootstrap state.
    GetBootstrapState,
    /// Get the transactions for the current bootstrap state.
    GetBootstrapTransactions,
}

/// Responses from the bootstrap HTTP server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BootstrapResponse {
    /// The current bootstrap state.
    BootstrapState(BootstrapState),
    /// The transactions for the current bootstrap state.
    BootstrapTransactions(Vec<RpcTransaction>),
}
