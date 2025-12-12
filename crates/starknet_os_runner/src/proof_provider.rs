use starknet_api::block::BlockNumber;
use starknet_rust::providers::jsonrpc::HttpTransport;
use starknet_rust::providers::{JsonRpcClient, Provider};
use starknet_rust_core::types::{ConfirmedBlockId, ContractStorageKeys, Felt, StorageProof};

use crate::errors::ProofProviderError;

#[allow(dead_code)]
pub trait ProofProvider {
    async fn get_starknet_proof(
        &self,
        block_number: BlockNumber,
        class_hashes: &[Felt],
        contract_addresses: &[Felt],
        contract_storage_keys: &[ContractStorageKeys],
    ) -> Result<StorageProof, ProofProviderError>;
}

impl ProofProvider for JsonRpcClient<HttpTransport> {
    async fn get_starknet_proof(
        &self,
        block_number: BlockNumber,
        class_hashes: &[Felt],
        contract_addresses: &[Felt],
        contract_storage_keys: &[ContractStorageKeys],
    ) -> Result<StorageProof, ProofProviderError> {
        let block_id = ConfirmedBlockId::Number(block_number.0);
        let proof = self
            .get_storage_proof(block_id, class_hashes, contract_addresses, contract_storage_keys)
            .await?;
        Ok(proof)
    }
}
