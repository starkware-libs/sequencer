#[cfg(test)]
mod test;

use apollo_class_manager_types::transaction_converter::TransactionConverterTrait;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use apollo_mempool_p2p_types::communication::{
    MempoolP2pPropagatorRequest,
    MempoolP2pPropagatorResponse,
};
use apollo_mempool_p2p_types::errors::MempoolP2pPropagatorError;
use apollo_mempool_p2p_types::mempool_p2p_types::MempoolP2pPropagatorResult;
use apollo_metrics::metrics::LossyIntoF64;
use apollo_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::mempool::RpcTransactionBatch;
use async_trait::async_trait;
use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use tracing::{debug, info, warn};

use crate::metrics::MEMPOOL_P2P_BROADCASTED_BATCH_SIZE;

pub struct MempoolP2pPropagator {
    broadcast_topic_client: BroadcastTopicClient<RpcTransactionBatch>,
    transaction_converter: Box<dyn TransactionConverterTrait + Send>,
    max_transaction_batch_size: usize,
    transaction_queue: Vec<RpcTransaction>,
}

impl MempoolP2pPropagator {
    pub fn new(
        broadcast_topic_client: BroadcastTopicClient<RpcTransactionBatch>,
        transaction_converter: Box<dyn TransactionConverterTrait + Send>,
        max_transaction_batch_size: usize,
    ) -> Self {
        MEMPOOL_P2P_BROADCASTED_BATCH_SIZE.register();
        Self {
            broadcast_topic_client,
            transaction_converter,
            max_transaction_batch_size,
            transaction_queue: vec![],
        }
    }
}

#[async_trait]
impl ComponentRequestHandler<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse>
    for MempoolP2pPropagator
{
    async fn handle_request(
        &mut self,
        request: MempoolP2pPropagatorRequest,
    ) -> MempoolP2pPropagatorResponse {
        match request {
            MempoolP2pPropagatorRequest::AddTransaction(transaction) => {
                MempoolP2pPropagatorResponse::AddTransaction(
                    self.add_transaction(transaction).await,
                )
            }
            MempoolP2pPropagatorRequest::ContinuePropagation(broadcasted_message_metadata) => {
                MempoolP2pPropagatorResponse::ContinuePropagation(
                    self.continue_propagation(broadcasted_message_metadata).await,
                )
            }
            MempoolP2pPropagatorRequest::BroadcastQueuedTransactions() => {
                MempoolP2pPropagatorResponse::BroadcastQueuedTransactions(
                    self.broadcast_queued_transactions().await,
                )
            }
        }
    }
}

impl MempoolP2pPropagator {
    async fn add_transaction(
        &mut self,
        transaction: InternalRpcTransaction,
    ) -> MempoolP2pPropagatorResult<()> {
        info!("Received a new transaction to broadcast to other mempool peers");
        debug!("broadcasted tx_hash: {:?}", transaction.tx_hash);
        let transaction =
            match self.transaction_converter.convert_internal_rpc_tx_to_rpc_tx(transaction).await {
                Ok(transaction) => transaction,
                Err(err) => {
                    debug!("Error converting transaction: {:?}", err);
                    return Err(MempoolP2pPropagatorError::TransactionConversionError(
                        err.to_string(),
                    ));
                }
            };

        self.transaction_queue.push(transaction);
        if self.transaction_queue.len() == self.max_transaction_batch_size {
            info!("Transaction batch is full. Broadcasting the transaction batch");
            return self.broadcast_queued_transactions().await;
        }
        Ok(())
    }

    async fn continue_propagation(
        &mut self,
        broadcasted_message_metadata: BroadcastedMessageMetadata,
    ) -> MempoolP2pPropagatorResult<()> {
        info!("Continuing propagation of received transaction");
        debug!("Propagated transaction metadata: {:?}", broadcasted_message_metadata);
        self.broadcast_topic_client
            .continue_propagation(&broadcasted_message_metadata)
            .await
            .map_err(|_| MempoolP2pPropagatorError::NetworkSendError)
    }

    async fn broadcast_queued_transactions(&mut self) -> MempoolP2pPropagatorResult<()> {
        let queued_transactions: Vec<RpcTransaction> = self.transaction_queue.drain(..).collect();
        if queued_transactions.is_empty() {
            return Ok(());
        }
        let number_of_transactions_in_batch = queued_transactions.len().into_f64();
        let result = self
            .broadcast_topic_client
            .broadcast_message(RpcTransactionBatch(queued_transactions))
            .await
            .or_else(|err| {
                if !err.is_full() {
                    warn!("Error broadcasting transaction batch: {:?}", err);
                    return Err(MempoolP2pPropagatorError::NetworkSendError);
                }
                warn!(
                    "Trying to send a transaction batch to other mempool peers but the buffer is \
                     full. Dropping the transaction batch."
                );
                Ok(())
            });
        MEMPOOL_P2P_BROADCASTED_BATCH_SIZE.record(number_of_transactions_in_batch);
        result
    }
}

pub type LocalMempoolP2pPropagatorServer = LocalComponentServer<
    MempoolP2pPropagator,
    MempoolP2pPropagatorRequest,
    MempoolP2pPropagatorResponse,
>;
pub type RemoteMempoolP2pPropagatorServer =
    RemoteComponentServer<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse>;

impl ComponentStarter for MempoolP2pPropagator {}
