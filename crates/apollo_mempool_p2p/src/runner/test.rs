use std::sync::Arc;
use std::time::Duration;

use apollo_gateway_types::communication::{GatewayClient, GatewayClientError, MockGatewayClient};
use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_gateway_types::errors::GatewayError;
use apollo_gateway_types::gateway_types::{GatewayInput, GatewayOutput, InvokeGatewayOutput};
use apollo_infra::component_definitions::ComponentStarter;
use apollo_mempool_p2p_types::communication::{
    MempoolP2pPropagatorClient,
    MockMempoolP2pPropagatorClient,
};
use apollo_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use apollo_network::network_manager::{BroadcastTopicChannels, NetworkError};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::mempool::RpcTransactionBatch;
use apollo_test_utils::{get_rng, GetTestInstance};
use futures::future::{pending, ready, BoxFuture};
use futures::stream::StreamExt;
use futures::{FutureExt, SinkExt};
use starknet_api::core::Nonce;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;

use super::MempoolP2pRunner;

const MAX_TRANSACTION_BATCH_RATE: Duration = Duration::MAX;
const MAX_CONCURRENT_GATEWAY_REQUESTS: usize = 10000;

fn setup(
    network_future: BoxFuture<'static, Result<(), NetworkError>>,
    gateway_client: Arc<dyn GatewayClient>,
    mempool_p2p_propagator_client: Arc<dyn MempoolP2pPropagatorClient>,
    transaction_batch_rate_millis: Duration,
    max_concurrent_gateway_requests: usize,
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
        mempool_p2p_propagator_client,
        transaction_batch_rate_millis,
        max_concurrent_gateway_requests,
    );
    (mempool_p2p_runner, mock_network)
}

#[test]
#[should_panic]
fn run_panics_when_network_future_returns() {
    let network_future = ready(Ok(())).boxed();
    let gateway_client = Arc::new(MockGatewayClient::new());
    let (mut mempool_p2p_runner, _) = setup(
        network_future,
        gateway_client,
        Arc::new(MockMempoolP2pPropagatorClient::new()),
        MAX_TRANSACTION_BATCH_RATE,
        MAX_CONCURRENT_GATEWAY_REQUESTS,
    );
    mempool_p2p_runner.start().now_or_never().unwrap();
}

#[test]
#[should_panic]
fn run_panics_when_network_future_returns_error() {
    let network_future =
        ready(Err(NetworkError::DialError(libp2p::swarm::DialError::Aborted))).boxed();
    let gateway_client = Arc::new(MockGatewayClient::new());
    let (mut mempool_p2p_runner, _) = setup(
        network_future,
        gateway_client,
        Arc::new(MockMempoolP2pPropagatorClient::new()),
        MAX_TRANSACTION_BATCH_RATE,
        MAX_CONCURRENT_GATEWAY_REQUESTS,
    );
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
            Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(TransactionHash::default())))
        },
    );
    let (mut mempool_p2p_runner, mock_network) = setup(
        network_future,
        Arc::new(mock_gateway_client),
        Arc::new(MockMempoolP2pPropagatorClient::new()),
        MAX_TRANSACTION_BATCH_RATE,
        MAX_CONCURRENT_GATEWAY_REQUESTS,
    );

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
        Err(GatewayClientError::GatewayError(GatewayError::DeprecatedGatewayError {
            source: StarknetError {
                code: StarknetErrorCode::KnownErrorCode(
                    KnownStarknetErrorCode::DuplicatedTransaction,
                ),
                message: format!("Transaction with hash {} already exists.", Nonce::default()),
            },
            p2p_message_metadata: Some(message_metadata_clone),
        }))
    });

    let (mut mempool_p2p_runner, mock_network) = setup(
        network_future,
        Arc::new(mock_gateway_client),
        Arc::new(MockMempoolP2pPropagatorClient::new()),
        MAX_TRANSACTION_BATCH_RATE,
        MAX_CONCURRENT_GATEWAY_REQUESTS,
    );

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

#[tokio::test]
async fn send_broadcast_queued_transactions_request_after_transaction_batch_rate() {
    let transaction_batch_rate_millis = Duration::from_secs(30);

    let network_future = pending().boxed();
    let gateway_client = Arc::new(MockGatewayClient::new());

    // Create channels for sending an empty message to indicate that the request reached the
    // propagator client
    let (broadcast_queued_tx_indicator_sender, mut broadcast_queued_tx_indicator_receiver) =
        futures::channel::oneshot::channel();

    let mut mempool_p2p_propagator_client = MockMempoolP2pPropagatorClient::new();
    mempool_p2p_propagator_client.expect_broadcast_queued_transactions().return_once(move || {
        broadcast_queued_tx_indicator_sender.send(()).unwrap();
        Ok(())
    });

    let (mut mempool_p2p_runner, _) = setup(
        network_future,
        gateway_client,
        Arc::new(mempool_p2p_propagator_client),
        transaction_batch_rate_millis,
        MAX_CONCURRENT_GATEWAY_REQUESTS,
    );

    tokio::time::pause();

    let handle = tokio::spawn(async move {
        mempool_p2p_runner.start().await;
    });

    // The event for which we want to advance the clock is polled by a tokio::select, and not
    // directly awaited.
    // Because of that, advancing the clock using tokio::time::advance won't actually push the
    // time forward (thanks to tokio lazy implementation). This is why we need to await a future
    // that will actually advance the clock (the clock is advanced without calling
    // tokio::time::advance thanks to the auto-advance feature of tokio::time::pause).
    // The auto-advance feature will instantly push the clock to the next awaited future, in this
    // case pushing it exactly to when a batch should be closed.
    tokio::time::sleep(transaction_batch_rate_millis * 2).await;

    assert!(broadcast_queued_tx_indicator_receiver.try_recv().unwrap().is_some());

    handle.abort();
}

#[tokio::test]
async fn reject_transaction_due_to_backpressure() {
    const NUM_CONCURRENT_GATEWAY_REQUESTS: usize = 1;
    // Because of the backpressure, the second transaction in this batch should be rejected.
    let rng = &mut get_rng();
    let rpc_transaction_batch_1 = RpcTransactionBatch(vec![
        RpcTransaction::get_test_instance(rng),
        RpcTransaction::get_test_instance(rng),
    ]);
    let rpc_transaction_batch_2 = RpcTransactionBatch(vec![RpcTransaction::get_test_instance(rng)]);

    let message_metadata = BroadcastedMessageMetadata::get_test_instance(rng);

    let gateway_input_1 = GatewayInput {
        rpc_tx: rpc_transaction_batch_1.0.first().unwrap().clone(),
        message_metadata: Some(message_metadata.clone()),
    };
    let gateway_input_2 = GatewayInput {
        rpc_tx: rpc_transaction_batch_2.0.first().unwrap().clone(),
        message_metadata: Some(message_metadata.clone()),
    };

    let mut mock_gateway_client = MockGatewayClient::new();

    let message_metadata_clone = message_metadata.clone();
    // We return an error here so we can ensure the gateway client's response was received by the
    // mempool p2p runner before sending the second tx batch.
    mock_gateway_client.expect_add_tx().with(mockall::predicate::eq(gateway_input_1)).return_once(
        move |_| {
            Err(GatewayClientError::GatewayError(GatewayError::DeprecatedGatewayError {
                source: StarknetError {
                    code: StarknetErrorCode::KnownErrorCode(
                        KnownStarknetErrorCode::DuplicatedTransaction,
                    ),
                    message: "Mock error".to_string(),
                },
                p2p_message_metadata: Some(message_metadata_clone),
            }))
        },
    );
    let (add_tx_indicator_sender, add_tx_indicator_receiver) = futures::channel::oneshot::channel();
    mock_gateway_client.expect_add_tx().with(mockall::predicate::eq(gateway_input_2)).return_once(
        move |_| {
            add_tx_indicator_sender.send(()).unwrap();
            Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(TransactionHash::default())))
        },
    );

    let (mut mempool_p2p_runner, mock_network) = setup(
        pending().boxed(),
        Arc::new(mock_gateway_client),
        Arc::new(MockMempoolP2pPropagatorClient::new()),
        MAX_TRANSACTION_BATCH_RATE,
        NUM_CONCURRENT_GATEWAY_REQUESTS,
    );
    let runner_handle = tokio::spawn(async move {
        mempool_p2p_runner.start().await;
    });
    let BroadcastNetworkMock {
        broadcasted_messages_sender: mut mock_broadcasted_messages_sender,
        reported_messages_receiver: mut mock_reported_messages_receiver,
        ..
    } = mock_network;

    tokio::time::timeout(Duration::from_secs(5), async move {
        mock_broadcasted_messages_sender
            .send((rpc_transaction_batch_1.clone(), message_metadata.clone()))
            .await
            .expect("Failed to send message");
        mock_reported_messages_receiver.next().await.expect("Failed to receive report");

        mock_broadcasted_messages_sender
            .send((rpc_transaction_batch_2.clone(), message_metadata.clone()))
            .await
            .expect("Failed to send message");
        add_tx_indicator_receiver.await.expect("add_tx_indicator_sender dropped");
    })
    .await
    .expect("Timeout waiting for transactions to be added to the gateway client");

    runner_handle.abort();
}
