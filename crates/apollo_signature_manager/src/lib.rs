use apollo_signature_manager_types::SignatureManagerResult;
use starknet_api::crypto::utils::{Message, PublicKey, RawSignature};

/// Provides signing and signature verification functionality.
pub struct SignatureManager;

impl SignatureManager {
    pub async fn sign(&self, _message: Message) -> SignatureManagerResult<RawSignature> {
        unimplemented!("SignatureManager::sign is not implemented");
    }

    pub async fn verify(
        &self,
        _signature: RawSignature,
        _message: Message,
        _public_key: PublicKey,
    ) -> SignatureManagerResult<bool> {
        unimplemented!("SignatureManager::verify is not implemented");
    }
}
