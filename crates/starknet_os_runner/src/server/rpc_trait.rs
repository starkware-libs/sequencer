//! JSON-RPC trait definition for the proving service.
//!
//! Defines the `ProvingRpc` trait using jsonrpsee's `#[rpc]` macro.

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use starknet_api::rpc_transaction::RpcTransaction;

use crate::virtual_snos_prover::ProveTransactionResult;

/// JSON-RPC trait for the proving service.
///
/// Namespace: `starknet` (methods will be prefixed with `starknet_`).
#[rpc(server, namespace = "starknet")]
pub trait ProvingRpc {
    /// Returns the spec version (serves as lightweight health check).
    ///
    /// Returns "0.10.0" for Starknet RPC v0.10 compatibility.
    #[method(name = "specVersion")]
    async fn spec_version(&self) -> RpcResult<String>;

    /// Proves a transaction on top of the specified block.
    ///
    /// # Parameters
    /// - `block_id`: The block to execute the transaction on.
    /// - `transaction`: The transaction to prove (must be an Invoke transaction).
    ///
    /// # Returns
    /// The proof, proof facts, and L2-to-L1 messages.
    #[method(name = "proveTransaction")]
    async fn prove_transaction(
        &self,
        block_id: BlockId,
        transaction: RpcTransaction,
    ) -> RpcResult<ProveTransactionResult>;
}
