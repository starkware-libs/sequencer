use std::sync::Arc;
use std::time::Duration;

use futures::future::{pending, ready, BoxFuture};
use futures::stream::StreamExt;
use futures::{FutureExt, SinkExt};
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkError};
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::mempool::RpcTransactionBatch;
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::{GatewayClient, GatewayClientError, MockGatewayClient};
use starknet_gateway_types::errors::{GatewayError, GatewaySpecError};
use starknet_gateway_types::gateway_types::GatewayInput;
use starknet_mempool_p2p_types::communication::MockMempoolP2pPropagatorClient;
use starknet_sequencer_infra::component_definitions::ComponentStarter;

use super::MempoolP2pRunner;

fn setup(
    network_future: BoxFuture<'static, Result<(), NetworkError>>,
    gateway_client: Arc<dyn GatewayClient>,
) -> (MempoolP2pRunner, BroadcastNetworkMock<RpcTransactionBatch>) {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        subscriber_channels;
    let mempool_p2p_runner = MempoolP2pRunner::new(
        network_future,
        broadcasted_messages_receiver,
        broadcast_topic_client,
        gateway_client,
        Arc::new(MockMempoolP2pPropagatorClient::new()),
    );
    (mempool_p2p_runner, mock_network)
}

#[test]
#[should_panic]
fn run_panics_when_network_future_returns() {
    let network_future = ready(Ok(())).boxed();
    let gateway_client = Arc::new(MockGatewayClient::new());
    let (mut mempool_p2p_runner, _) = setup(network_future, gateway_client);
    mempool_p2p_runner.start().now_or_never().unwrap();
}

#[test]
#[should_panic]
fn run_panics_when_network_future_returns_error() {
    let network_future =
        ready(Err(NetworkError::DialError(libp2p::swarm::DialError::Aborted))).boxed();
    let gateway_client = Arc::new(MockGatewayClient::new());
    let (mut mempool_p2p_runner, _) = setup(network_future, gateway_client);
    mempool_p2p_runner.start().now_or_never().unwrap();
}

#[tokio::test]
async fn incoming_p2p_tx_reaches_gateway_client() {
    let network_future = pending().boxed();

    // Create channels for sending an empty message to indicate that the tx reached the gateway
    // client.
    let (add_tx_indicator_sender, add_tx_indicator_receiver) = futures::channel::oneshot::channel();

    let message_metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    let expected_rpc_transaction_batch =
        RpcTransactionBatch(vec![RpcTransaction::get_test_instance(&mut get_rng())]);
    let gateway_input = GatewayInput {
        rpc_tx: expected_rpc_transaction_batch.0.first().unwrap().clone(),
        message_metadata: Some(message_metadata.clone()),
    };

    let mut mock_gateway_client = MockGatewayClient::new();
    mock_gateway_client.expect_add_tx().with(mockall::predicate::eq(gateway_input)).return_once(
        move |_| {
            add_tx_indicator_sender.send(()).unwrap();
            Ok(TransactionHash::default())
        },
    );
    let (mut mempool_p2p_runner, mock_network) =
        setup(network_future, Arc::new(mock_gateway_client));

    let BroadcastNetworkMock {
        broadcasted_messages_sender: mut mock_broadcasted_messages_sender,
        ..
    } = mock_network;

    let res = mock_broadcasted_messages_sender
        .send((expected_rpc_transaction_batch.clone(), message_metadata));

    res.await.expect("Failed to send message");

    tokio::select! {
        // if the runner fails, there was a network issue => panic.
        // if the runner returns successfully, we panic because the runner should never terminate.
        res = tokio::time::timeout(Duration::from_secs(5), mempool_p2p_runner.start()) => {
            res.expect("Test timed out");
            panic!("MempoolP2pRunner terminated");
        },
        // if a message was received on this oneshot channel, the gateway client received the tx and the test succeeded.
        res = add_tx_indicator_receiver => {res.unwrap()}
    }
}

// The p2p runner receives a tx from network, and the gateway declines it, triggering report_peer.
#[tokio::test]
async fn incoming_p2p_tx_fails_on_gateway_client() {
    let network_future = pending().boxed();
    // Create channels for sending an empty message to indicate that the tx reached the gateway
    // client.
    let (add_tx_indicator_sender, add_tx_indicator_receiver) = futures::channel::oneshot::channel();

    let message_metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    let message_metadata_clone = message_metadata.clone();
    let expected_rpc_transaction_batch =
        RpcTransactionBatch(vec![RpcTransaction::get_test_instance(&mut get_rng())]);

    let mut mock_gateway_client = MockGatewayClient::new();
    mock_gateway_client.expect_add_tx().return_once(move |_| {
        add_tx_indicator_sender.send(()).unwrap();
        Err(GatewayClientError::GatewayError(GatewayError::GatewaySpecError {
            source: GatewaySpecError::DuplicateTx,
            p2p_message_metadata: Some(message_metadata_clone),
        }))
    });

    let (mut mempool_p2p_runner, mock_network) =
        setup(network_future, Arc::new(mock_gateway_client));

    let BroadcastNetworkMock {
        broadcasted_messages_sender: mut mock_broadcasted_messages_sender,
        reported_messages_receiver: mut mock_reported_messages_receiver,
        ..
    } = mock_network;

    let res = mock_broadcasted_messages_sender
        .send((expected_rpc_transaction_batch.clone(), message_metadata.clone()));

    res.await.expect("Failed to send message");

    tokio::select! {
        // if the runner fails, there was a network issue => panic.
        // if the runner returns successfully, we panic because the runner should never terminate.
        res = tokio::time::timeout(Duration::from_secs(5), mempool_p2p_runner.start()) => {
            res.expect("Test timed out (MempoolP2pRunner took too long to start)");
            panic!("MempoolP2pRunner terminated");
        },
        // if a message was received on this oneshot channel, the gateway client received the tx.
        res = add_tx_indicator_receiver => {
            // if unwrap fails, the tx wasn't forwarded to the gateway client.
            res.unwrap();
            // After gateway client fails to add the tx, the p2p runner should have reported the peer.
            let peer_reported = mock_reported_messages_receiver.next().await.expect("Failed to receive report");
            // TODO(Shahak): add this functionality to network manager test utils
            assert_eq!(peer_reported, message_metadata.originator_id.private_get_peer_id())
        }
    }
}
