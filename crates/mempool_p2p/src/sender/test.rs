use futures::stream::StreamExt;
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_network_types::network_types::BroadcastedMessageManager;
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_p2p_types::communication::MempoolP2pSenderRequest;
use tokio::time::timeout;

use crate::sender::MempoolP2pSender;

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

#[tokio::test]
async fn process_handle_add_tx() {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver: _, broadcast_topic_client } =
        subscriber_channels;
    let BroadcastNetworkMock { mut messages_to_broadcast_receiver, .. } = mock_network;
    let rpc_transaction = RpcTransaction::get_test_instance(&mut get_rng());
    let mut mempool_sender = MempoolP2pSender::new(broadcast_topic_client);
    mempool_sender
        .handle_request(MempoolP2pSenderRequest::AddTransaction(rpc_transaction.clone()))
        .await;
    let message = timeout(TIMEOUT, messages_to_broadcast_receiver.next()).await.unwrap().unwrap();
    assert_eq!(message, RpcTransactionWrapper(rpc_transaction));
}

#[tokio::test]
async fn process_handle_continue_propagation() {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver: _, broadcast_topic_client } =
        subscriber_channels;
    let BroadcastNetworkMock { mut continue_propagation_receiver, .. } = mock_network;
    let propagation_manager = BroadcastedMessageManager::get_test_instance(&mut get_rng());
    let mut mempool_sender = MempoolP2pSender::new(broadcast_topic_client);
    mempool_sender
        .handle_request(MempoolP2pSenderRequest::ContinuePropagation(propagation_manager.clone()))
        .await;
    let message = timeout(TIMEOUT, continue_propagation_receiver.next()).await.unwrap().unwrap();
    assert_eq!(message, propagation_manager);
}
