use std::net::IpAddr;
use std::sync::Arc;

use async_trait::async_trait;
use papyrus_network::network_manager::BroadcastedMessageManager;
use starknet_api::executable_transaction::Transaction;
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use starknet_mempool_p2p::sender::EmptyMempoolP2pSenderClient;
use starknet_mempool_p2p_sender_types::communication::SharedMempoolP2pSenderClient;
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

pub fn create_mempool_server(
    mempool: Mempool,
    rx_mempool: Receiver<MempoolRequestAndResponseSender>,
) -> MempoolServer {
    let communication_wrapper =
        MempoolCommunicationWrapper::new(mempool, Arc::new(EmptyMempoolP2pSenderClient));
    LocalComponentServer::new(communication_wrapper, rx_mempool)
}

pub fn create_remote_mempool_server(
    mempool: Mempool,
    ip_address: IpAddr,
    port: u16,
) -> RemoteMempoolServer {
    let communication_wrapper =
        MempoolCommunicationWrapper::new(mempool, Arc::new(EmptyMempoolP2pSenderClient));
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

    async fn send_tx_to_p2p(
        &self,
        message_metadata: Option<BroadcastedMessageManager>,
        tx: Transaction,
    ) {
        match message_metadata {
            Some(message_metadata) => {
                self.mempool_p2p_sender_client
                    .continue_propagation(message_metadata)
                    .await
                    .unwrap();
            }
            None => {
                self.mempool_p2p_sender_client.add_transaction(tx.into()).await.unwrap();
            }
        }
    }

    async fn add_tx(&mut self, mempool_wrapper_input: MempoolWrapperInput) -> MempoolResult<()> {
        let result = self.mempool.add_tx(mempool_wrapper_input.mempool_input.clone());
        self.send_tx_to_p2p(
            mempool_wrapper_input.message_metadata,
            mempool_wrapper_input.mempool_input.tx,
        )
        .await;
        result
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
