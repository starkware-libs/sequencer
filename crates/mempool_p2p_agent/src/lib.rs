use std::future::pending;
use std::sync::Arc;

use async_trait::async_trait;
pub use papyrus_network::network_manager::BroadcastedMessageManager;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_mempool_infra::component_client::{ClientError, ClientResult, LocalComponentClient};
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_runner::{ComponentStartError, ComponentStarter};

pub struct _MempoolP2PAgent;

// TODO: In gateway, use this instead of MempoolClient.
#[async_trait]
pub trait MempoolP2PAgentClient: Send + Sync {
    /// Adds a transaction to be propagated to other peers. If this transaction came from another
    /// peer, the `propagation_manager` argument should contain details how to continue the
    /// propagation.
    async fn add_transaction(
        &self,
        transaction: RpcTransaction,
        propagation_manager: Option<BroadcastedMessageManager>,
    ) -> ClientResult<()>;

    /// Gets all requests for adding transactions we received from other peers to the mempool. If
    /// there are no requests this function will wait until there's at least a single request
    async fn get_add_transaction_requests(
        &self,
    ) -> ClientResult<Vec<(RpcTransaction, BroadcastedMessageManager)>>;

    /// Report that an attempt to add transaction through a call to get_add_transaction_requests
    /// failed.
    async fn report_error_on_add_transaction_request(
        &self,
        propagation_manager: BroadcastedMessageManager,
    ) -> ClientResult<()>;
}

pub type SharedMempoolP2PAgentClient = Arc<dyn MempoolP2PAgentClient>;

#[async_trait]
impl ComponentStarter for _MempoolP2PAgent {
    async fn start(&mut self) -> Result<(), ComponentStartError> {
        // TODO: implement this and remove the pending.
        let () = pending().await;
        Ok(())
    }
}

pub enum MempoolP2PAgentRequest {
    AddTransaction(RpcTransaction, Option<BroadcastedMessageManager>),
    GetAddTransactionRequests,
    ReportErrorOnAddTransactionRequest(BroadcastedMessageManager),
}

pub enum MempoolP2PAgentResponse {
    AddTransaction,
    GetAddTransactionRequests(Vec<(RpcTransaction, BroadcastedMessageManager)>),
    ReportErrorOnAddTransactionRequest,
}

#[async_trait]
impl ComponentRequestHandler<MempoolP2PAgentRequest, MempoolP2PAgentResponse> for _MempoolP2PAgent {
    async fn handle_request(
        &mut self,
        _request: MempoolP2PAgentRequest,
    ) -> MempoolP2PAgentResponse {
        unimplemented!()
    }
}

#[async_trait]
impl MempoolP2PAgentClient
    for LocalComponentClient<MempoolP2PAgentRequest, MempoolP2PAgentResponse>
{
    async fn add_transaction(
        &self,
        transaction: RpcTransaction,
        propagation_manager: Option<BroadcastedMessageManager>,
    ) -> ClientResult<()> {
        let res = self
            .send(MempoolP2PAgentRequest::AddTransaction(transaction, propagation_manager))
            .await;
        match res {
            MempoolP2PAgentResponse::AddTransaction => Ok(()),
            MempoolP2PAgentResponse::GetAddTransactionRequests(_) => {
                unexpected_response_type_error("AddTransaction", "GetAddTransactionRequests")
            }
            MempoolP2PAgentResponse::ReportErrorOnAddTransactionRequest => {
                unexpected_response_type_error(
                    "AddTransaction",
                    "ReportErrorOnAddTransactionRequest",
                )
            }
        }
    }

    async fn get_add_transaction_requests(
        &self,
    ) -> ClientResult<Vec<(RpcTransaction, BroadcastedMessageManager)>> {
        let res = self.send(MempoolP2PAgentRequest::GetAddTransactionRequests).await;
        match res {
            MempoolP2PAgentResponse::GetAddTransactionRequests(requests) => Ok(requests),
            MempoolP2PAgentResponse::AddTransaction => {
                unexpected_response_type_error("GetAddTransactionRequests", "AddTransaction")
            }
            MempoolP2PAgentResponse::ReportErrorOnAddTransactionRequest => {
                unexpected_response_type_error(
                    "GetAddTransactionRequests",
                    "ReportErrorOnAddTransactionRequest",
                )
            }
        }
    }

    async fn report_error_on_add_transaction_request(
        &self,
        propagation_manager: BroadcastedMessageManager,
    ) -> ClientResult<()> {
        let res = self
            .send(MempoolP2PAgentRequest::ReportErrorOnAddTransactionRequest(propagation_manager))
            .await;
        match res {
            MempoolP2PAgentResponse::ReportErrorOnAddTransactionRequest => Ok(()),
            MempoolP2PAgentResponse::AddTransaction => unexpected_response_type_error(
                "ReportErrorOnAddTransactionRequest",
                "AddTransaction",
            ),
            MempoolP2PAgentResponse::GetAddTransactionRequests(_) => {
                unexpected_response_type_error(
                    "ReportErrorOnAddTransactionRequest",
                    "GetAddTransactionRequests",
                )
            }
        }
    }
}

fn unexpected_response_type_error<T>(expected_type: &str, actual_type: &str) -> ClientResult<T> {
    Err(ClientError::UnexpectedResponse(format!(
        "Expected response of type {expected_type}, got {actual_type}"
    )))
}
