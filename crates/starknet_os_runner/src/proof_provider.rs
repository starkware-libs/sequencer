//! Proof provider trait for fetching proofs from RPC.

use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_rust::providers::jsonrpc::HttpTransport;
use starknet_rust::providers::{JsonRpcClient, Provider};
use starknet_rust_core::types::{ConfirmedBlockId, ContractStorageKeys, Felt};

use crate::errors::ProofProviderError;
use crate::os_input_builder::RpcProofResult;

/// Trait for fetching and converting proofs from an RPC endpoint.
#[allow(async_fn_in_trait)]
pub trait ProofProvider {
    /// Fetch proofs for the given block and convert to `RpcProofResult`.
    ///
    /// # Arguments
    /// * `block_number` - Block number to fetch proofs for.
    /// * `class_hashes` - Class hashes to include in the proof.
    /// * `contract_addresses` - Contract addresses to include in the proof.
    /// * `contract_storage_keys` - Storage keys per contract to include in the proof.
    ///
    /// # Returns
    /// `RpcProofResult` containing forest proofs and state roots.
    async fn get_proofs(
        &self,
        block_number: BlockNumber,
        class_hashes: &[Felt],
        contract_addresses: &[ContractAddress],
        contract_storage_keys: &[ContractStorageKeys],
    ) -> Result<RpcProofResult, ProofProviderError>;
}

impl ProofProvider for JsonRpcClient<HttpTransport> {
    async fn get_proofs(
        &self,
        block_number: BlockNumber,
        class_hashes: &[Felt],
        contract_addresses: &[ContractAddress],
        contract_storage_keys: &[ContractStorageKeys],
    ) -> Result<RpcProofResult, ProofProviderError> {
        let block_id = ConfirmedBlockId::Number(block_number.0);

        // Convert ContractAddress to Felt for the RPC call.
        let address_felts: Vec<Felt> = contract_addresses.iter().map(|a| *a.key()).collect();

        let storage_proof = self
            .get_storage_proof(block_id, class_hashes, &address_felts, contract_storage_keys)
            .await?;

        Ok(RpcProofResult::from_storage_proof(storage_proof, contract_addresses))
    }
}
