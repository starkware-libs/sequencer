use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::channel::mpsc::Sender;
use futures::stream::StreamExt;
use futures::SinkExt;
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_network::NetworkConfig;
use papyrus_network_types::network_types::BroadcastedMessageManager;
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::{GatewayClient, GatewayClientResult};
use starknet_gateway_types::gateway_types::GatewayInput;
use starknet_mempool_infra::component_definitions::ComponentStarter;
use tokio::time::sleep;

use crate::receiver::MempoolP2pReceiver;

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
    let mock_network_manager = NetworkManager::new(NetworkConfig::default(), None);
    let (sender, mut receiver) = futures::channel::mpsc::channel(1);
    let mock_gateway_client = Arc::new(MockGatewayClient { add_tx_sender: sender });
    let mut mempool_receiver = MempoolP2pReceiver::new(
        Some(mock_network_manager),
        broadcasted_messages_receiver,
        broadcast_topic_client,
        mock_gateway_client,
    );
    let mut rng = get_rng();
    let broadcasted_message_manager = BroadcastedMessageManager::get_test_instance(&mut rng);
    let expected_rpc_transaction =
        RpcTransactionWrapper(RpcTransaction::get_test_instance(&mut rng));
    let res = mock_broadcasted_messages_sender
        .send((expected_rpc_transaction.clone(), broadcasted_message_manager));

    res.await.expect("Failed to send message");
    tokio::select! {
        _ = mempool_receiver.start() => {panic!("Mempool receiver failed to start");}
        actual_rpc_transaction = receiver.next() => {
            assert_eq!(actual_rpc_transaction, Some(expected_rpc_transaction.0));
        }
        _ = sleep(Duration::from_secs(5)) => {
            panic!("Test timed out");
        }
    }
}
