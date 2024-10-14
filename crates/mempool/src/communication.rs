use async_trait::async_trait;
use starknet_api::executable_transaction::Transaction;
use starknet_mempool_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use starknet_mempool_infra::component_server::LocalComponentServer;
use starknet_mempool_types::communication::{
    AddTransactionArgsWrapper,
    MempoolRequest,
    MempoolRequestAndResponseSender,
    MempoolResponse,
};
use starknet_mempool_types::mempool_types::{CommitBlockArgs, MempoolResult};
use tokio::sync::mpsc::Receiver;

use crate::mempool::Mempool;

pub type LocalMempoolServer =
    LocalComponentServer<MempoolCommunicationWrapper, MempoolRequest, MempoolResponse>;

pub fn create_mempool_server(
    mempool: Mempool,
    rx_mempool: Receiver<MempoolRequestAndResponseSender>,
) -> LocalMempoolServer {
    let communication_wrapper = MempoolCommunicationWrapper::new(mempool);
    LocalComponentServer::new(communication_wrapper, rx_mempool)
}

/// Wraps the mempool to enable inbound async communication from other components.
pub struct MempoolCommunicationWrapper {
    mempool: Mempool,
}

impl MempoolCommunicationWrapper {
    pub fn new(mempool: Mempool) -> Self {
        MempoolCommunicationWrapper { mempool }
    }

    fn add_tx(&mut self, args_wrapper: AddTransactionArgsWrapper) -> MempoolResult<()> {
        self.mempool.add_tx(args_wrapper.args)
    }

    fn commit_block(&mut self, args: CommitBlockArgs) -> MempoolResult<()> {
        self.mempool.commit_block(args)
    }

    fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<Transaction>> {
        self.mempool.get_txs(n_txs)
    }
}

#[async_trait]
impl ComponentRequestHandler<MempoolRequest, MempoolResponse> for MempoolCommunicationWrapper {
    async fn handle_request(&mut self, request: MempoolRequest) -> MempoolResponse {
        match request {
            MempoolRequest::AddTransaction(args) => {
                MempoolResponse::AddTransaction(self.add_tx(args))
            }
            MempoolRequest::CommitBlock(args) => {
                MempoolResponse::CommitBlock(self.commit_block(args))
            }
            MempoolRequest::GetTransactions(n_txs) => {
                MempoolResponse::GetTransactions(self.get_txs(n_txs))
            }
        }
    }
}

impl ComponentStarter for MempoolCommunicationWrapper {}
