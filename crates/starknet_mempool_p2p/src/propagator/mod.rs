#[cfg(test)]
mod test;

use async_trait::async_trait;
use papyrus_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use starknet_class_manager_types::transaction_converter::{
    TransactionConverter,
    TransactionConverterTrait,
};
use starknet_mempool_p2p_types::communication::{
    MempoolP2pPropagatorRequest,
    MempoolP2pPropagatorResponse,
};
use starknet_mempool_p2p_types::errors::MempoolP2pPropagatorError;
use starknet_sequencer_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use starknet_sequencer_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use tracing::warn;

pub struct MempoolP2pPropagator {
    broadcast_topic_client: BroadcastTopicClient<RpcTransactionWrapper>,
    transaction_converter: TransactionConverter,
}

impl MempoolP2pPropagator {
    pub fn new(
        broadcast_topic_client: BroadcastTopicClient<RpcTransactionWrapper>,
        transaction_converter: TransactionConverter,
    ) -> Self {
        Self { broadcast_topic_client, transaction_converter }
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

                let result = self
                    .broadcast_topic_client
                    .broadcast_message(RpcTransactionWrapper(transaction))
                    .await
                    .or_else(|err| {
                        if !err.is_full() {
                            return Err(MempoolP2pPropagatorError::NetworkSendError);
                        }
                        warn!(
                            "Trying to send a transaction to other mempool peers but the buffer \
                             is full. Dropping the transaction."
                        );
                        Ok(())
                    });
                MempoolP2pPropagatorResponse::AddTransaction(result)
            }
            MempoolP2pPropagatorRequest::ContinuePropagation(propagation_manager) => {
                let result = self
                    .broadcast_topic_client
                    .continue_propagation(&propagation_manager)
                    .await
                    .map_err(|_| MempoolP2pPropagatorError::NetworkSendError);
                MempoolP2pPropagatorResponse::ContinuePropagation(result)
            }
        }
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
