use apollo_network_types::network_types::PeerId;
use apollo_signature_manager::LocalKeyStoreSignatureManager;
use apollo_signature_manager_types::{SignatureManagerClient, SignatureManagerClientResult};
use async_trait::async_trait;
use starknet_api::block::BlockHash;
use starknet_api::crypto::utils::{Challenge, RawSignature};

/// Adapter that implements [`SignatureManagerClient`] by delegating directly to a
/// [`LocalKeyStoreSignatureManager`], bypassing the component-client infrastructure.
///
/// Use this when the actual StarkID signer is not available and a temporary, local-only
/// signer is needed (e.g. during development or when authentication is not configured).
pub(crate) struct DirectSignatureManagerClient(pub LocalKeyStoreSignatureManager);

#[async_trait]
impl SignatureManagerClient for DirectSignatureManagerClient {
    async fn sign_identification(
        &self,
        peer_id: PeerId,
        challenge: Challenge,
    ) -> SignatureManagerClientResult<RawSignature> {
        Ok(self.0.sign_identification(peer_id, challenge).await?)
    }

    async fn sign_precommit_vote(
        &self,
        block_hash: BlockHash,
    ) -> SignatureManagerClientResult<RawSignature> {
        Ok(self.0.sign_precommit_vote(block_hash).await?)
    }
}
