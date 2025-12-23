use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use apollo_proof_manager_types::{ProofManagerError, ProofManagerRequest, ProofManagerResponse};
use async_trait::async_trait;

use crate::proof_manager::ProofManager;
use crate::proof_storage::ProofStorage;
pub type LocalProofManagerServer =
    ConcurrentLocalComponentServer<ProofManager, ProofManagerRequest, ProofManagerResponse>;
pub type RemoteProofManagerServer =
    RemoteComponentServer<ProofManagerRequest, ProofManagerResponse>;

#[async_trait]
impl ComponentRequestHandler<ProofManagerRequest, ProofManagerResponse> for ProofManager {
    async fn handle_request(&mut self, request: ProofManagerRequest) -> ProofManagerResponse {
        match request {
            ProofManagerRequest::SetProof(facts_hash, proof) => ProofManagerResponse::SetProof(
                self.set_proof(facts_hash, proof)
                    .map_err(|e| ProofManagerError::ProofStorage(e.to_string())),
            ),
            ProofManagerRequest::GetProof(facts_hash) => ProofManagerResponse::GetProof(
                self.get_proof(facts_hash)
                    .map_err(|e| ProofManagerError::ProofStorage(e.to_string())),
            ),
            ProofManagerRequest::ContainsProof(facts_hash) => ProofManagerResponse::ContainsProof(
                self.contains_proof(facts_hash)
                    .map_err(|e| ProofManagerError::ProofStorage(e.to_string())),
            ),
        }
    }
}
