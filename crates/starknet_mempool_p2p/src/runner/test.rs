use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::channel::mpsc::Sender;
use futures::future::{ready, BoxFuture};
use futures::stream::StreamExt;
use futures::{FutureExt, SinkExt};
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkError, NetworkManager};
use papyrus_network::NetworkConfig;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::{GatewayClient, GatewayClientResult};
use starknet_gateway_types::gateway_types::GatewayInput;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use tokio::time::sleep;

use super::MempoolP2pRunner;

fn setup(
    network_future: BoxFuture<'static, Result<(), NetworkError>>,
    gateway_client: Arc<dyn GatewayClient>,
) -> MempoolP2pRunner {
    let TestSubscriberChannels { mock_network: _, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        subscriber_channels;
    MempoolP2pRunner::new(
        network_future,
        broadcasted_messages_receiver,
        broadcast_topic_client,
        gateway_client,
    )
}

#[test]
fn run_returns_when_network_future_returns() {
    let network_future = ready(Ok(())).boxed();
    let gateway_client =
        Arc::new(MockGatewayClient { add_tx_sender: futures::channel::mpsc::channel(1).0 });
    let mut mempool_p2p_runner = setup(network_future, gateway_client);
    mempool_p2p_runner.start().now_or_never().unwrap().unwrap();
}

#[test]
fn run_returns_error_when_network_future_returns_error() {
    let network_future =
        ready(Err(NetworkError::DialError(libp2p::swarm::DialError::Aborted))).boxed();
    let gateway_client =
        Arc::new(MockGatewayClient { add_tx_sender: futures::channel::mpsc::channel(1).0 });
    let mut mempool_p2p_runner = setup(network_future, gateway_client);
    mempool_p2p_runner.start().now_or_never().unwrap().unwrap_err();
}

// TODO(eitan): Make it an automock
#[derive(Clone)]
struct MockGatewayClient {
    add_tx_sender: Sender<RpcTransaction>,
}

#[async_trait]
impl GatewayClient for MockGatewayClient {
    async fn add_tx(&self, gateway_input: GatewayInput) -> GatewayClientResult<TransactionHash> {
        let _ = self.clone().add_tx_sender.send(gateway_input.rpc_tx).await;
        Ok(TransactionHash::default())
    }
}

#[tokio::test]
async fn start_component_receive_tx_happy_flow() {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        subscriber_channels;
    let BroadcastNetworkMock {
        broadcasted_messages_sender: mut mock_broadcasted_messages_sender,
        ..
    } = mock_network;
    // Creating a placeholder network manager with default config for init of a mempool receiver
    let placeholder_network_manager = NetworkManager::new(NetworkConfig::default(), None);
    let placeholder_network_future = placeholder_network_manager.run();
    let (add_tx_sender, mut add_tx_receiver) = futures::channel::mpsc::channel(1);
    let mock_gateway_client = Arc::new(MockGatewayClient { add_tx_sender });
    let mut mempool_p2p_runner = MempoolP2pRunner::new(
        Box::pin(placeholder_network_future),
        broadcasted_messages_receiver,
        broadcast_topic_client,
        mock_gateway_client,
    );
    let message_metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    let expected_rpc_transaction =
        RpcTransactionWrapper(RpcTransaction::get_test_instance(&mut get_rng()));

    // Sending the expected transaction to the mempool receiver
    let res =
        mock_broadcasted_messages_sender.send((expected_rpc_transaction.clone(), message_metadata));

    res.await.expect("Failed to send message");
    tokio::select! {
        _ = mempool_p2p_runner.start() => {panic!("Mempool receiver failed to start");}
        actual_rpc_transaction = add_tx_receiver.next() => {
            assert_eq!(actual_rpc_transaction, Some(expected_rpc_transaction.0));
        }
        _ = sleep(Duration::from_secs(5)) => {
            panic!("Test timed out");
        }
    }
}
// TODO(eitan): Add test for when the gateway client fails to add the transaction
