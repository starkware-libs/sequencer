use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, ComponentRequestAndResponseSender};
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHash;
use starknet_api::core::Nonce;
use starknet_api::crypto::utils::{PrivateKey, RawSignature, SignatureConversionError};
use strum_macros::AsRefStr;
use thiserror::Error;

pub type KeyStoreResult<T> = Result<T, KeyStoreError>;
pub type SignatureManagerResult<T> = Result<T, SignatureManagerError>;
pub type SignatureManagerClientResult<T> = Result<T, SignatureManagerClientError>;

pub type LocalSignatureManagerClient =
    LocalComponentClient<SignatureManagerRequest, SignatureManagerResponse>;
pub type RemoteSignatureManagerClient =
    RemoteComponentClient<SignatureManagerRequest, SignatureManagerResponse>;

pub type SharedSignatureManagerClient = Arc<dyn SignatureManagerClient>;
pub type SignatureManagerRequestAndResponseSender =
    ComponentRequestAndResponseSender<SignatureManagerRequest, SignatureManagerResponse>;

/// A read-only key store that contains exactly one key.
#[async_trait]
pub trait KeyStore: Clone + Send + Sync {
    /// Retrieve a reference to the contained private key.
    async fn get_key(&self) -> KeyStoreResult<PrivateKey>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum KeyStoreError {
    #[error("Failed to fetch key: {0}")]
    Custom(String),
}

#[derive(
    Clone, Debug, Default, derive_more::Deref, Eq, PartialEq, Hash, Serialize, Deserialize,
)]
pub struct PeerId(pub Vec<u8>);

impl From<Vec<u8>> for PeerId {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

/// Serves as the signature manager's shared interface.
/// Requires `Send + Sync` to allow transferring and sharing resources (inputs, futures) across
/// threads.
#[async_trait]
#[cfg_attr(any(feature = "testing", test), automock)]
pub trait SignatureManagerClient: Send + Sync {
    async fn identify(
        &self,
        peer_id: PeerId,
        nonce: Nonce,
    ) -> SignatureManagerClientResult<RawSignature>;

    async fn sign_precommit_vote(
        &self,
        block_hash: BlockHash,
    ) -> SignatureManagerClientResult<RawSignature>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum SignatureManagerError {
    #[error("Internal client error: {0}")]
    Client(String),
    #[error(transparent)]
    KeyStore(#[from] KeyStoreError),
    #[error("Failed to sign: {0}")]
    Sign(String),
    #[error(transparent)]
    SignatureConversion(#[from] SignatureConversionError),
    #[error("Failed to verify: {0}")]
    Verify(String),
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
    Identify(PeerId, Nonce),
    SignPrecommitVote(BlockHash),
}
impl_debug_for_infra_requests_and_responses!(SignatureManagerRequest);

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum SignatureManagerResponse {
    Identify(SignatureManagerResult<RawSignature>),
    SignPrecommitVote(SignatureManagerResult<RawSignature>),
}
impl_debug_for_infra_requests_and_responses!(SignatureManagerResponse);

#[async_trait]
impl<ComponentClientType> SignatureManagerClient for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<SignatureManagerRequest, SignatureManagerResponse>,
{
    async fn identify(
        &self,
        peer_id: PeerId,
        nonce: Nonce,
    ) -> SignatureManagerClientResult<RawSignature> {
        let request = SignatureManagerRequest::Identify(peer_id, nonce);
        handle_all_response_variants!(
            SignatureManagerResponse,
            Identify,
            SignatureManagerClientError,
            SignatureManagerError,
            Direct
        )
    }

    async fn sign_precommit_vote(
        &self,
        block_hash: BlockHash,
    ) -> SignatureManagerClientResult<RawSignature> {
        let request = SignatureManagerRequest::SignPrecommitVote(block_hash);
        handle_all_response_variants!(
            SignatureManagerResponse,
            SignPrecommitVote,
            SignatureManagerClientError,
            SignatureManagerError,
            Direct
        )
    }
}
