use apollo_signature_manager_types::SignatureManagerResult;
use starknet_api::crypto::utils::{Message, PublicKey, RawSignature};

/// Provides signing and signature verification functionality.
pub struct SignatureManager<KS: KeyStore> {
    pub keystore: KS,
}

impl<KS: KeyStore> SignatureManager<KS> {
    fn _new(keystore: KS) -> Self {
        Self { keystore }
    }

    pub async fn sign(&self, _message: Message) -> SignatureManagerResult<RawSignature> {
        todo!("SignatureManager::sign is not yet implemented");
    }

    pub async fn verify(
        &self,
        _signature: RawSignature,
        _message: Message,
        _public_key: PublicKey,
    ) -> SignatureManagerResult<bool> {
        todo!("SignatureManager::verify is not yet implemented");
    }
}
