use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use apollo_proof_manager_types::{ProofManagerError, ProofManagerRequest, ProofManagerResponse};
use async_trait::async_trait;

use crate::proof_manager::ProofManager;
pub type LocalProofManagerServer =
    ConcurrentLocalComponentServer<ProofManager, ProofManagerRequest, ProofManagerResponse>;
pub type RemoteProofManagerServer =
    RemoteComponentServer<ProofManagerRequest, ProofManagerResponse>;

#[async_trait]
impl ComponentRequestHandler<ProofManagerRequest, ProofManagerResponse> for ProofManager {
    async fn handle_request(&mut self, request: ProofManagerRequest) -> ProofManagerResponse {
        match request {
            ProofManagerRequest::SetProof(proof_facts, nonce, sender_address, proof) => {
                ProofManagerResponse::SetProof(
                    self.set_proof(proof_facts, nonce, sender_address, proof)
                        .map_err(|e| ProofManagerError::ProofStorage(e.to_string())),
                )
            }
            ProofManagerRequest::GetProof(proof_facts, nonce, sender_address) => {
                ProofManagerResponse::GetProof(
                    self.get_proof(proof_facts, nonce, sender_address)
                        .map_err(|e| ProofManagerError::ProofStorage(e.to_string())),
                )
            }
            ProofManagerRequest::ContainsProof(proof_facts, nonce, sender_address) => {
                ProofManagerResponse::ContainsProof(
                    self.contains_proof(proof_facts, nonce, sender_address)
                        .map_err(|e| ProofManagerError::ProofStorage(e.to_string())),
                )
            }
        }
    }
}
