use std::sync::Arc;

use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use apollo_mempool_p2p_types::communication::SharedMempoolP2pPropagatorClient;
use apollo_mempool_types::communication::{
    AddTransactionArgsWrapper,
    MempoolRequest,
    MempoolResponse,
};
use apollo_mempool_types::errors::MempoolError;
use apollo_mempool_types::mempool_types::{CommitBlockArgs, MempoolResult, MempoolSnapshot};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_time::time::DefaultClock;
use async_trait::async_trait;
use starknet_api::block::GasPrice;
use starknet_api::core::ContractAddress;
use starknet_api::rpc_transaction::InternalRpcTransaction;
use tracing::warn;

use crate::config::MempoolConfig;
use crate::mempool::Mempool;
use crate::metrics::register_metrics;

pub type LocalMempoolServer =
    LocalComponentServer<MempoolCommunicationWrapper, MempoolRequest, MempoolResponse>;
pub type RemoteMempoolServer = RemoteComponentServer<MempoolRequest, MempoolResponse>;

pub fn create_mempool(
    config: MempoolConfig,
    mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
) -> MempoolCommunicationWrapper {
    MempoolCommunicationWrapper::new(
        Mempool::new(config, Arc::new(DefaultClock)),
        mempool_p2p_propagator_client,
    )
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
        if let Err(p2p_client_err) =
            self.send_tx_to_p2p(args_wrapper.p2p_message_metadata, args_wrapper.args.tx).await
        {
            warn!("Failed to send transaction to P2P: {:?}", p2p_client_err);
        }
        Ok(())
    }

    fn commit_block(&mut self, args: CommitBlockArgs) -> MempoolResult<()> {
        self.mempool.commit_block(args);
        Ok(())
    }

    fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<InternalRpcTransaction>> {
        self.mempool.get_txs(n_txs)
    }

    fn account_tx_in_pool_or_recent_block(
        &self,
        account_address: ContractAddress,
    ) -> MempoolResult<bool> {
        Ok(self.mempool.account_tx_in_pool_or_recent_block(account_address))
    }

    fn update_gas_price(&mut self, gas_price: GasPrice) -> MempoolResult<()> {
        self.mempool.update_gas_price(gas_price);
        Ok(())
    }

    fn mempool_snapshot(&self) -> MempoolResult<MempoolSnapshot> {
        self.mempool.mempool_snapshot()
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
            MempoolRequest::AccountTxInPoolOrRecentBlock(account_address) => {
                MempoolResponse::AccountTxInPoolOrRecentBlock(
                    self.account_tx_in_pool_or_recent_block(account_address),
                )
            }
            MempoolRequest::UpdateGasPrice(gas_price) => {
                MempoolResponse::UpdateGasPrice(self.update_gas_price(gas_price))
            }
            MempoolRequest::GetMempoolSnapshot() => {
                MempoolResponse::GetMempoolSnapshot(self.mempool_snapshot())
            }
        }
    }
}

#[async_trait]
impl ComponentStarter for MempoolCommunicationWrapper {
    async fn start(&mut self) {
        register_metrics();
    }
}
