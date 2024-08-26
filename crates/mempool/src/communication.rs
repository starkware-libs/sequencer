use std::net::IpAddr;

use async_trait::async_trait;
use starknet_api::executable_transaction::Transaction;
use starknet_mempool_infra::component_definitions::{ComponentMonitor, ComponentRequestHandler};
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_infra::component_server::{LocalComponentServer, RemoteComponentServer};
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
    let communication_wrapper = MempoolCommunicationWrapper::new(mempool);
    LocalComponentServer::new(communication_wrapper, rx_mempool)
}

pub fn create_remote_mempool_server(
    mempool: Mempool,
    ip_address: IpAddr,
    port: u16,
) -> RemoteMempoolServer {
    let communication_wrapper = MempoolCommunicationWrapper::new(mempool);
    RemoteComponentServer::new(communication_wrapper, ip_address, port)
}

/// Wraps the mempool to enable inbound async communication from other components.
pub struct MempoolCommunicationWrapper {
    mempool: Mempool,
}

impl MempoolCommunicationWrapper {
    pub fn new(mempool: Mempool) -> Self {
        MempoolCommunicationWrapper { mempool }
    }

    fn add_tx(&mut self, mempool_wrapper_input: MempoolWrapperInput) -> MempoolResult<()> {
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
                MempoolResponse::AddTransaction(self.add_tx(mempool_wrapper_input))
            }
            MempoolRequest::GetTransactions(n_txs) => {
                MempoolResponse::GetTransactions(self.get_txs(n_txs))
            }
        }
    }
}

#[async_trait]
impl ComponentStarter for MempoolCommunicationWrapper {}

#[async_trait]
impl ComponentMonitor for MempoolCommunicationWrapper {}
