use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest, RequestWrapper};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_infra::{
    handle_all_response_variants,
    impl_debug_for_infra_requests_and_responses,
    impl_labeled_request,
};
use apollo_metrics::generate_permutation_labels;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::fields::{Proof, ProofFacts};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};
use thiserror::Error;

pub type ProofManagerResult<T> = Result<T, ProofManagerError>;
pub type ProofManagerClientResult<T> = Result<T, ProofManagerClientError>;

pub type LocalProofManagerClient = LocalComponentClient<ProofManagerRequest, ProofManagerResponse>;
pub type RemoteProofManagerClient =
    RemoteComponentClient<ProofManagerRequest, ProofManagerResponse>;
pub type ProofManagerRequestWrapper = RequestWrapper<ProofManagerRequest, ProofManagerResponse>;

pub type SharedProofManagerClient = Arc<dyn ProofManagerClient>;

/// Serves as the proof manager's shared interface.
/// Requires `Send + Sync` to allow transferring and sharing resources across threads.
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait ProofManagerClient: Send + Sync {
    async fn set_proof(
        &self,
        proof_facts: ProofFacts,
        nonce: Nonce,
        sender_address: ContractAddress,
        proof: Proof,
    ) -> ProofManagerClientResult<()>;

    async fn get_proof(
        &self,
        proof_facts: ProofFacts,
        nonce: Nonce,
        sender_address: ContractAddress,
    ) -> ProofManagerClientResult<Option<Proof>>;

    async fn contains_proof(
        &self,
        proof_facts: ProofFacts,
        nonce: Nonce,
        sender_address: ContractAddress,
    ) -> ProofManagerClientResult<bool>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProofManagerError {
    #[error("Internal client error: {0}")]
    Client(String),
    #[error("Proof storage error: {0}")]
    ProofStorage(String),
    #[error("IO error: {0}")]
    Io(String),
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum ProofManagerClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    ProofManagerError(#[from] ProofManagerError),
}

#[derive(Clone, Serialize, Deserialize, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(ProofManagerRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum ProofManagerRequest {
    SetProof(ProofFacts, Nonce, ContractAddress, Proof),
    GetProof(ProofFacts, Nonce, ContractAddress),
    ContainsProof(ProofFacts, Nonce, ContractAddress),
}
impl_debug_for_infra_requests_and_responses!(ProofManagerRequest);
impl_labeled_request!(ProofManagerRequest, ProofManagerRequestLabelValue);
impl PrioritizedRequest for ProofManagerRequest {}

generate_permutation_labels! {
    PROOF_MANAGER_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, ProofManagerRequestLabelValue),
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum ProofManagerResponse {
    SetProof(ProofManagerResult<()>),
    GetProof(ProofManagerResult<Option<Proof>>),
    ContainsProof(ProofManagerResult<bool>),
}
impl_debug_for_infra_requests_and_responses!(ProofManagerResponse);

#[async_trait]
impl<ComponentClientType> ProofManagerClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<ProofManagerRequest, ProofManagerResponse>,
{
    async fn set_proof(
        &self,
        proof_facts: ProofFacts,
        nonce: Nonce,
        sender_address: ContractAddress,
        proof: Proof,
    ) -> ProofManagerClientResult<()> {
        let request = ProofManagerRequest::SetProof(proof_facts, nonce, sender_address, proof);
        handle_all_response_variants!(
            self,
            request,
            ProofManagerResponse,
            SetProof,
            ProofManagerClientError,
            ProofManagerError,
            Direct
        )
    }

    async fn get_proof(
        &self,
        proof_facts: ProofFacts,
        nonce: Nonce,
        sender_address: ContractAddress,
    ) -> ProofManagerClientResult<Option<Proof>> {
        let request = ProofManagerRequest::GetProof(proof_facts, nonce, sender_address);
        handle_all_response_variants!(
            self,
            request,
            ProofManagerResponse,
            GetProof,
            ProofManagerClientError,
            ProofManagerError,
            Direct
        )
    }

    async fn contains_proof(
        &self,
        proof_facts: ProofFacts,
        nonce: Nonce,
        sender_address: ContractAddress,
    ) -> ProofManagerClientResult<bool> {
        let request = ProofManagerRequest::ContainsProof(proof_facts, nonce, sender_address);
        handle_all_response_variants!(
            self,
            request,
            ProofManagerResponse,
            ContainsProof,
            ProofManagerClientError,
            ProofManagerError,
            Direct
        )
    }
}
