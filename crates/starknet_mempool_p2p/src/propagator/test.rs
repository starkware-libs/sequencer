use futures::stream::StreamExt;
use mockall::predicate;
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::mempool::RpcTransactionBatch;
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use starknet_class_manager_types::transaction_converter::MockTransactionConverterTrait;
use starknet_mempool_p2p_types::communication::MempoolP2pPropagatorRequest;
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use tokio::time::timeout;

use super::MempoolP2pPropagator;

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

#[tokio::test]
async fn process_handle_add_tx() {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver: _, broadcast_topic_client } =
        subscriber_channels;
    let BroadcastNetworkMock { mut messages_to_broadcast_receiver, .. } = mock_network;
    let internal_tx = InternalRpcTransaction::get_test_instance(&mut get_rng());
    let rpc_transaction = RpcTransaction::get_test_instance(&mut get_rng());
    let cloned_rpc_transaction = rpc_transaction.clone();
    let mut transaction_converter = MockTransactionConverterTrait::new();
    transaction_converter
        .expect_convert_internal_rpc_tx_to_rpc_tx()
        .with(predicate::eq(internal_tx.clone()))
        .times(1)
        .return_once(move |_| Ok(rpc_transaction));
    let mut mempool_p2p_propagator =
        MempoolP2pPropagator::new(broadcast_topic_client, Box::new(transaction_converter));
    mempool_p2p_propagator
        .handle_request(MempoolP2pPropagatorRequest::AddTransaction(internal_tx))
        .await;
    let message = timeout(TIMEOUT, messages_to_broadcast_receiver.next()).await.unwrap().unwrap();
    assert_eq!(message, RpcTransactionBatch(vec![cloned_rpc_transaction]));
}

#[tokio::test]
async fn process_handle_continue_propagation() {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver: _, broadcast_topic_client } =
        subscriber_channels;
    let BroadcastNetworkMock { mut continue_propagation_receiver, .. } = mock_network;
    let propagation_metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    let transaction_converter = MockTransactionConverterTrait::new();
    let mut mempool_p2p_propagator =
        MempoolP2pPropagator::new(broadcast_topic_client, Box::new(transaction_converter));
    mempool_p2p_propagator
        .handle_request(MempoolP2pPropagatorRequest::ContinuePropagation(
            propagation_metadata.clone(),
        ))
        .await;
    let message = timeout(TIMEOUT, continue_propagation_receiver.next()).await.unwrap().unwrap();
    assert_eq!(message, propagation_metadata);
}
