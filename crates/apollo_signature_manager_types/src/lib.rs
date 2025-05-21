use std::sync::Arc;

use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::ComponentClient;
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::crypto::utils::{Message, PublicKey, RawSignature};
use strum_macros::AsRefStr;
use thiserror::Error;

pub type SignatureManagerResult<T> = Result<T, SignatureManagerError>;
pub type SignatureManagerClientResult<T> = Result<T, SignatureManagerClientError>;

pub type SharedSignatureManagerClient = Arc<dyn SignatureManagerClient>;

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

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum SignatureManagerRequest {
    Sign(Message),
    Verify(RawSignature, Message, PublicKey),
}
impl_debug_for_infra_requests_and_responses!(SignatureManagerRequest);

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum SignatureManagerResponse {
    Sign(SignatureManagerResult<RawSignature>),
    Verify(SignatureManagerResult<bool>),
}
impl_debug_for_infra_requests_and_responses!(SignatureManagerResponse);

#[async_trait]
impl<ComponentClientType> SignatureManagerClient for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<SignatureManagerRequest, SignatureManagerResponse>,
{
    async fn sign(&self, message: Message) -> SignatureManagerClientResult<RawSignature> {
        let request = SignatureManagerRequest::Sign(message);
        handle_all_response_variants!(
            SignatureManagerResponse,
            Sign,
            SignatureManagerClientError,
            SignatureManagerError,
            Direct
        )
    }

    async fn verify(
        &self,
        signature: RawSignature,
        message: Message,
        public_key: PublicKey,
    ) -> SignatureManagerClientResult<bool> {
        let request = SignatureManagerRequest::Verify(signature, message, public_key);
        handle_all_response_variants!(
            SignatureManagerResponse,
            Verify,
            SignatureManagerClientError,
            SignatureManagerError,
            Direct
        )
    }
}
