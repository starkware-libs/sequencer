use std::net::IpAddr;
use std::sync::Arc;

use async_trait::async_trait;
use starknet_api::executable_transaction::Transaction;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use starknet_mempool_p2p::sender::{
    BroadcastedMessageManager,
    MempoolP2pSenderClient,
    MempoolP2pSenderClientResult,
    SharedMempoolP2pSenderClient,
};
use starknet_mempool_types::communication::{
    MempoolRequest,
    MempoolRequestAndResponseSender,
    MempoolResponse,
    MempoolWrapperInput,
};
use starknet_mempool_types::mempool_types::MempoolResult;
use tokio::sync::mpsc::Receiver;

use crate::mempool::Mempool;

pub type MempoolServer =
    LocalComponentServer<MempoolCommunicationWrapper, MempoolRequest, MempoolResponse>;

pub type RemoteMempoolServer =
    RemoteComponentServer<MempoolCommunicationWrapper, MempoolRequest, MempoolResponse>;

// TODO: remove this.
pub struct EmptyMempoolClient;

#[async_trait]
impl MempoolP2pSenderClient for EmptyMempoolClient {
    async fn add_transaction(
        &self,
        _transaction: RpcTransaction,
    ) -> MempoolP2pSenderClientResult<()> {
        Ok(())
    }

    async fn continue_propagation(
        &self,
        _propagation_manager: BroadcastedMessageManager,
    ) -> MempoolP2pSenderClientResult<()> {
        Ok(())
    }
}

pub fn create_mempool_server(
    mempool: Mempool,
    rx_mempool: Receiver<MempoolRequestAndResponseSender>,
) -> MempoolServer {
    let communication_wrapper =
        MempoolCommunicationWrapper::new(mempool, Arc::new(EmptyMempoolClient));
    LocalComponentServer::new(communication_wrapper, rx_mempool)
}

pub fn create_remote_mempool_server(
    mempool: Mempool,
    ip_address: IpAddr,
    port: u16,
) -> RemoteMempoolServer {
    let communication_wrapper =
        MempoolCommunicationWrapper::new(mempool, Arc::new(EmptyMempoolClient));
    RemoteComponentServer::new(communication_wrapper, ip_address, port)
}

/// Wraps the mempool to enable inbound async communication from other components.
pub struct MempoolCommunicationWrapper {
    mempool: Mempool,
    mempool_p2p_sender_client: SharedMempoolP2pSenderClient,
}

impl MempoolCommunicationWrapper {
    pub fn new(mempool: Mempool, mempool_p2p_sender_client: SharedMempoolP2pSenderClient) -> Self {
        MempoolCommunicationWrapper { mempool, mempool_p2p_sender_client }
    }

    async fn add_tx(&mut self, mempool_wrapper_input: MempoolWrapperInput) -> MempoolResult<()> {
        match mempool_wrapper_input.message_metadata {
            Some(message_metadata) => {
                self.mempool_p2p_sender_client
                    .continue_propagation(message_metadata)
                    .await
                    .unwrap();
            }
            None => {
                self.mempool_p2p_sender_client
                    .add_transaction(mempool_wrapper_input.mempool_input.tx.clone().into())
                    .await
                    .unwrap();
            }
        }
        self.mempool.add_tx(mempool_wrapper_input.mempool_input)
    }

    fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<Transaction>> {
        self.mempool.get_txs(n_txs)
    }
}

#[async_trait]
impl ComponentRequestHandler<MempoolRequest, MempoolResponse> for MempoolCommunicationWrapper {
    async fn handle_request(&mut self, request: MempoolRequest) -> MempoolResponse {
        match request {
            MempoolRequest::AddTransaction(mempool_wrapper_input) => {
                MempoolResponse::AddTransaction(self.add_tx(mempool_wrapper_input).await)
            }
            MempoolRequest::GetTransactions(n_txs) => {
                MempoolResponse::GetTransactions(self.get_txs(n_txs))
            }
        }
    }
}

#[async_trait]
impl ComponentStarter for MempoolCommunicationWrapper {}
