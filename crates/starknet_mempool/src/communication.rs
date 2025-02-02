use async_trait::async_trait;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use starknet_api::block::NonzeroGasPrice;
use starknet_api::core::ContractAddress;
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_mempool_p2p_types::communication::SharedMempoolP2pPropagatorClient;
use starknet_mempool_types::communication::{
    AddTransactionArgsWrapper,
    MempoolRequest,
    MempoolResponse,
};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{CommitBlockArgs, MempoolResult};
use starknet_sequencer_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use starknet_sequencer_infra::component_server::{LocalComponentServer, RemoteComponentServer};

use crate::mempool::Mempool;

pub type LocalMempoolServer =
    LocalComponentServer<MempoolCommunicationWrapper, MempoolRequest, MempoolResponse>;
pub type RemoteMempoolServer = RemoteComponentServer<MempoolRequest, MempoolResponse>;

pub fn create_mempool(
    mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
) -> MempoolCommunicationWrapper {
    MempoolCommunicationWrapper::new(Mempool::default(), mempool_p2p_propagator_client)
}

/// Wraps the mempool to enable inbound async communication from other components.
pub struct MempoolCommunicationWrapper {
    mempool: Mempool,
    mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
}

impl MempoolCommunicationWrapper {
    pub fn new(
        mempool: Mempool,
        mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
    ) -> Self {
        MempoolCommunicationWrapper { mempool, mempool_p2p_propagator_client }
    }

    async fn send_tx_to_p2p(
        &self,
        message_metadata: Option<BroadcastedMessageMetadata>,
        tx: InternalRpcTransaction,
    ) -> MempoolResult<()> {
        match message_metadata {
            Some(message_metadata) => self
                .mempool_p2p_propagator_client
                .continue_propagation(message_metadata)
                .await
                .map_err(|_| MempoolError::P2pPropagatorClientError { tx_hash: tx.tx_hash }),
            None => {
                let tx_hash = tx.tx_hash;
                self.mempool_p2p_propagator_client
                    .add_transaction(tx)
                    .await
                    .map_err(|_| MempoolError::P2pPropagatorClientError { tx_hash })?;
                Ok(())
            }
        }
    }

    pub(crate) async fn add_tx(
        &mut self,
        args_wrapper: AddTransactionArgsWrapper,
    ) -> MempoolResult<()> {
        self.mempool.add_tx(args_wrapper.args.clone())?;
        // TODO(AlonH): Verify that only transactions that were added to the mempool are sent.
        self.send_tx_to_p2p(args_wrapper.p2p_message_metadata, args_wrapper.args.tx).await
    }

    fn commit_block(&mut self, args: CommitBlockArgs) -> MempoolResult<()> {
        self.mempool.commit_block(args);
        Ok(())
    }

    fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<InternalRpcTransaction>> {
        self.mempool.get_txs(n_txs)
    }

    fn contains_tx_from(&self, account_address: ContractAddress) -> MempoolResult<bool> {
        Ok(self.mempool.contains_tx_from(account_address))
    }

    fn update_gas_price(&mut self, gas_price: NonzeroGasPrice) -> MempoolResult<()> {
        self.mempool.update_gas_price(gas_price);
        Ok(())
    }
}

#[async_trait]
impl ComponentRequestHandler<MempoolRequest, MempoolResponse> for MempoolCommunicationWrapper {
    async fn handle_request(&mut self, request: MempoolRequest) -> MempoolResponse {
        match request {
            MempoolRequest::AddTransaction(args) => {
                MempoolResponse::AddTransaction(self.add_tx(args).await)
            }
            MempoolRequest::CommitBlock(args) => {
                MempoolResponse::CommitBlock(self.commit_block(args))
            }
            MempoolRequest::GetTransactions(n_txs) => {
                MempoolResponse::GetTransactions(self.get_txs(n_txs))
            }
            MempoolRequest::ContainsTransactionFrom(account_address) => {
                MempoolResponse::ContainsTransactionFrom(self.contains_tx_from(account_address))
            }
            MempoolRequest::UpdateGasPrice(gas_price) => {
                MempoolResponse::UpdateGasPrice(self.update_gas_price(gas_price))
            }
        }
    }
}

impl ComponentStarter for MempoolCommunicationWrapper {}
