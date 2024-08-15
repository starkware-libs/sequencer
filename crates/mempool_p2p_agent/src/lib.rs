use std::collections::HashMap;
use std::future::pending;
use std::sync::Arc;

use async_trait::async_trait;
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::TransactionHash;
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

// TODO: Add this struct to papyrus_network and use the one from there.
struct BroadcastMessageManager;
#[allow(dead_code)]
pub struct MempoolP2PAgent {
    transactions_paused_propagation: HashMap<TransactionHash, BroadcastMessageManager>,
    mempool_client: SharedMempoolClient,
    // TODO(shahak): implement gateway client and add it here.
}

#[async_trait]
impl ComponentStarter for MempoolP2PAgent {
    async fn start(&mut self) -> Result<(), ComponentStartError> {
        // TODO: implement this and remove the pending.
        let () = pending().await;
        Ok(())
    }
}

pub struct AddTransactionRequest(pub MempoolInput);

pub struct AddTransactionResponse(pub ClientResult<()>);

#[async_trait]
impl ComponentRequestHandler<AddTransactionRequest, AddTransactionResponse> for MempoolP2PAgent {
    async fn handle_request(&mut self, request: AddTransactionRequest) -> AddTransactionResponse {
        let (transaction_hash, _transaction) = extract_transaction_from_mempool_input(&request.0);
        self.mempool_client.add_tx(request.0).await.expect("Failed adding transaction to mempool");
        if self.transactions_paused_propagation.remove(&transaction_hash).is_none() {
            // TODO: propagate the transaction
            self.transactions_paused_propagation.insert(transaction_hash, BroadcastMessageManager);
        }
        AddTransactionResponse(Ok(()))
    }
}

fn extract_transaction_from_mempool_input(
    _mempool_input: &MempoolInput,
) -> (TransactionHash, Transaction) {
    unimplemented!()
}
