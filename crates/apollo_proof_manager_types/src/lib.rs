use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest, RequestWrapper};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use apollo_metrics::generate_permutation_labels;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::transaction::fields::Proof;
use starknet_types_core::felt::Felt;
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
/// Requires `Send + Sync` to allow transferring and sharing resources (inputs, futures) across
/// threads.
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait ProofManagerClient: Send + Sync {
    async fn set_proof(&self, facts_hash: Felt, proof: Proof) -> ProofManagerClientResult<()>;

    async fn get_proof(&self, facts_hash: Felt) -> ProofManagerClientResult<Option<Proof>>;

    async fn contains_proof(&self, facts_hash: Felt) -> ProofManagerClientResult<bool>;
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

#[derive(Clone, Debug, Error)]
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
    SetProof(Felt, Proof),
    GetProof(Felt),
    ContainsProof(Felt),
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
    async fn set_proof(&self, facts_hash: Felt, proof: Proof) -> ProofManagerClientResult<()> {
        let request = ProofManagerRequest::SetProof(facts_hash, proof);
        handle_all_response_variants!(
            ProofManagerResponse,
            SetProof,
            ProofManagerClientError,
            ProofManagerError,
            Direct
        )
    }

    async fn get_proof(&self, facts_hash: Felt) -> ProofManagerClientResult<Option<Proof>> {
        let request = ProofManagerRequest::GetProof(facts_hash);
        handle_all_response_variants!(
            ProofManagerResponse,
            GetProof,
            ProofManagerClientError,
            ProofManagerError,
            Direct
        )
    }

    async fn contains_proof(&self, facts_hash: Felt) -> ProofManagerClientResult<bool> {
        let request = ProofManagerRequest::ContainsProof(facts_hash);
        handle_all_response_variants!(
            ProofManagerResponse,
            ContainsProof,
            ProofManagerClientError,
            ProofManagerError,
            Direct
        )
    }
}
