use std::future::pending;
use std::sync::Arc;

use async_trait::async_trait;
use starknet_mempool_infra::component_client::ClientResult;
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_runner::{ComponentStartError, ComponentStarter};
use starknet_mempool_types::communication::SharedMempoolClient;
use starknet_mempool_types::mempool_types::MempoolInput;

// TODO: In gateway, use this instead of MempoolClient.
#[async_trait]
pub trait MempoolP2PAgentClient: Send + Sync {
    async fn add_transaction(&self, mempool_input: MempoolInput) -> ClientResult<()>;
}

pub type SharedMempoolP2PAgentClient = Arc<dyn MempoolP2PAgentClient>;

#[allow(dead_code)]
pub struct MempoolP2PAgent {
    mempool_client: SharedMempoolClient,
    // TODO(shahak): implement gateway client and add it here.
}

#[async_trait]
impl ComponentStarter for MempoolP2PAgent {
    async fn start(&mut self) -> Result<(), ComponentStartError> {
        // TODO: implement this and remove the pending.
        Ok(pending().await)
    }
}

pub struct AddTransactionRequest(pub MempoolInput);

pub struct AddTransactionResponse(pub ClientResult<()>);

#[async_trait]
impl ComponentRequestHandler<AddTransactionRequest, AddTransactionResponse> for MempoolP2PAgent {
    async fn handle_request(&mut self, _request: AddTransactionRequest) -> AddTransactionResponse {
        // TODO: implement this
        AddTransactionResponse(Ok(()))
    }
}
