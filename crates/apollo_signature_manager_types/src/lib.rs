use apollo_infra::component_client::ClientError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::crypto::utils::{Message, PublicKey, RawSignature};
use thiserror::Error;

pub type SignatureManagerResult<T> = Result<T, SignatureManagerError>;
pub type SignatureManagerClientResult<T> = Result<T, SignatureManagerClientError>;

/// Serves as the signature manager's shared interface.
/// Requires `Send + Sync` to allow transferring and sharing resources (inputs, futures) across
/// threads.
#[async_trait]
pub trait SignatureManagerClient: Send + Sync {
    async fn sign(&self, message: Message) -> SignatureManagerClientResult<RawSignature>;

    async fn verify(
        &self,
        signature: RawSignature,
        message: Message,
        public_key: PublicKey,
    ) -> SignatureManagerClientResult<bool>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum SignatureManagerError {
    #[error("Internal client error: {0}")]
    Client(String),
}

#[derive(Clone, Debug, Error)]
pub enum SignatureManagerClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    SignatureManagerError(#[from] SignatureManagerError),
}
