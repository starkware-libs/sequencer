#[cfg(test)]
mod test;

use async_trait::async_trait;
use papyrus_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use papyrus_protobuf::mempool::RpcTransactionBatch;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_class_manager_types::transaction_converter::TransactionConverterTrait;
use starknet_mempool_p2p_types::communication::{
    MempoolP2pPropagatorRequest,
    MempoolP2pPropagatorResponse,
};
use starknet_mempool_p2p_types::errors::MempoolP2pPropagatorError;
use starknet_mempool_p2p_types::mempool_p2p_types::MempoolP2pPropagatorResult;
use starknet_sequencer_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use starknet_sequencer_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use tracing::{debug, info, warn};

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
                info!("Received a new transaction to broadcast to other mempool peers");
                debug!("broadcasted tx_hash: {:?}", transaction.tx_hash);
                let transaction = match self
                    .transaction_converter
                    .convert_internal_rpc_tx_to_rpc_tx(transaction)
                    .await
                {
                    Ok(transaction) => transaction,
                    Err(err) => {
                        return MempoolP2pPropagatorResponse::AddTransaction(Err(
                            MempoolP2pPropagatorError::TransactionConversionError(err.to_string()),
                        ));
                    }
                };

                self.transaction_queue.push(transaction);
                if self.transaction_queue.len() == self.max_transaction_batch_size {
                    info!("Transaction batch is full. Broadcasting the transaction batch");
                    return MempoolP2pPropagatorResponse::AddTransaction(
                        self.broadcast_queued_transactions().await,
                    );
                }
                MempoolP2pPropagatorResponse::AddTransaction(Ok(()))
            }
            MempoolP2pPropagatorRequest::ContinuePropagation(propagation_manager) => {
                info!("Continuing propagation of received transaction");
                debug!("Propagated transaction metadata: {:?}", propagation_manager);
                let result = self
                    .broadcast_topic_client
                    .continue_propagation(&propagation_manager)
                    .await
                    .map_err(|_| MempoolP2pPropagatorError::NetworkSendError);
                MempoolP2pPropagatorResponse::ContinuePropagation(result)
            }
            MempoolP2pPropagatorRequest::BroadcastQueuedTransactions() => {
                if self.transaction_queue.is_empty() {
                    return MempoolP2pPropagatorResponse::BroadcastQueuedTransactions(Ok(()));
                }
                MempoolP2pPropagatorResponse::BroadcastQueuedTransactions(
                    self.broadcast_queued_transactions().await,
                )
            }
        }
    }
}

impl MempoolP2pPropagator {
    async fn broadcast_queued_transactions(&mut self) -> MempoolP2pPropagatorResult<()> {
        self.broadcast_topic_client
            .broadcast_message(RpcTransactionBatch(self.transaction_queue.drain(..).collect()))
            .await
            .or_else(|err| {
                if !err.is_full() {
                    return Err(MempoolP2pPropagatorError::NetworkSendError);
                }
                warn!(
                    "Trying to send a transaction batch to other mempool peers but the buffer is \
                     full. Dropping the transaction batch."
                );
                Ok(())
            })
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
