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
            ProofManagerRequest::SetProof(proof_facts, proof) => ProofManagerResponse::SetProof(
                self.set_proof(proof_facts, proof)
                    .map_err(|e| ProofManagerError::ProofStorage(e.to_string())),
            ),
            ProofManagerRequest::GetProof(proof_facts) => ProofManagerResponse::GetProof(
                self.get_proof(proof_facts)
                    .map_err(|e| ProofManagerError::ProofStorage(e.to_string())),
            ),
            ProofManagerRequest::ContainsProof(proof_facts) => ProofManagerResponse::ContainsProof(
                self.contains_proof(proof_facts)
                    .map_err(|e| ProofManagerError::ProofStorage(e.to_string())),
            ),
        }
    }
}
