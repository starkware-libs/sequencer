use std::sync::Arc;
use std::time::Duration;

use futures::SinkExt;
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_network::NetworkConfig;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::MockGatewayClient;
use starknet_sequencer_infra::component_definitions::ComponentStarter;

use super::MempoolP2pRunner;

// The p2p runner receives a tx from network, and forwards it to the gateway.
#[tokio::test]
async fn incoming_p2p_tx_reaches_gateway_client() {
    // Mock a network for the other node to send tx to our p2p runner using the subscriber channels.
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        subscriber_channels; // used to created our node's p2p runner below, which will listen for incoming txs over broadcasted_messages_receiver.
    let BroadcastNetworkMock {
        broadcasted_messages_sender: mut mock_broadcasted_messages_sender,
        ..
    } = mock_network; // other node sending tx to our p2p runner

    // Creating a placeholder network manager with default config for init of a mempool receiver
    let placeholder_network_manager = NetworkManager::new(NetworkConfig::default(), None);

    // send an empty message on this channel to indicate that the tx reached the gateway client.
    let (add_tx_indicator_sender, add_tx_indicator_receiver) = futures::channel::oneshot::channel();

    let mut mock_gateway_client = MockGatewayClient::new();
    mock_gateway_client.expect_add_tx().return_once(move |_| {
        add_tx_indicator_sender.send(()).unwrap();
        Ok(TransactionHash::default())
    });
    let mut mempool_p2p_runner = MempoolP2pRunner::new(
        Some(placeholder_network_manager),
        broadcasted_messages_receiver, // listen to incoming tx
        broadcast_topic_client,        // broadcast tx or report peer
        Arc::new(mock_gateway_client),
    );

    let message_metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    let expected_rpc_transaction =
        RpcTransactionWrapper(RpcTransaction::get_test_instance(&mut get_rng()));

    // Sending the expected transaction to the mempool runner
    let res =
        mock_broadcasted_messages_sender.send((expected_rpc_transaction.clone(), message_metadata));

    res.await.expect("Failed to send message");

    tokio::select! {
        // if the runner takes longer than 5 seconds to start, we panic.
        // if the runner fails, there was a network issue => panic.
        // if the runner returns successfully, we panic because the runner should never terminate.
        res = tokio::time::timeout(Duration::from_secs(5), mempool_p2p_runner.start()) => {res.expect("Test timed out").expect("Runner failed - network stopped unexpectedly"); panic!("Runner terminated")},
        // if a message was received on this oneshot channel, the gateway client received the tx.
        res = add_tx_indicator_receiver => {res.unwrap()}
    }
}
