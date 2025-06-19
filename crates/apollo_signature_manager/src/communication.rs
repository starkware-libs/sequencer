use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use apollo_signature_manager_types::{SignatureManagerRequest, SignatureManagerResponse};
use async_trait::async_trait;

use crate::SignatureManager;

pub type LocalSignatureManagerServer = ConcurrentLocalComponentServer<
    SignatureManager,
    SignatureManagerRequest,
    SignatureManagerResponse,
>;
pub type RemoteSignatureManagerServer =
    RemoteComponentServer<SignatureManagerRequest, SignatureManagerResponse>;

#[async_trait]
impl ComponentRequestHandler<SignatureManagerRequest, SignatureManagerResponse>
    for SignatureManager
{
    async fn handle_request(
        &mut self,
        request: SignatureManagerRequest,
    ) -> SignatureManagerResponse {
        match request {
            SignatureManagerRequest::Identify(peer_id, nonce) => {
                SignatureManagerResponse::Identify(self.identify(peer_id, nonce).await)
            }
            SignatureManagerRequest::SignPrecommitVote(block_hash) => {
                SignatureManagerResponse::SignPrecommitVote(
                    self.sign_precommit_vote(block_hash).await,
                )
            }
        }
    }
}
