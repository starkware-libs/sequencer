use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use mempool_infra::network_component::CommunicationInterface;
use rstest::rstest;
use starknet_api::transaction::{Tip, TransactionHash};
use starknet_gateway::config::{
    GatewayConfig, GatewayNetworkConfig, StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway::gateway::Gateway;
use starknet_gateway::gateway_client;
use starknet_gateway::starknet_api_test_utils::invoke_tx;
use starknet_gateway::state_reader_test_utils::test_state_reader_factory;
use starknet_mempool::mempool::Mempool;
use starknet_mempool_types::mempool_types::{
    BatcherToMempoolChannels, BatcherToMempoolMessage, GatewayNetworkComponent,
    GatewayToMempoolMessage, MempoolInput, MempoolNetworkComponent, MempoolToBatcherMessage,
    MempoolToGatewayMessage,
};
use tokio::sync::mpsc::channel;
use tokio::task;
use tokio::time::sleep;

#[tokio::test]
async fn test_send_and_receive() {
    let (tx_gateway_to_mempool, rx_gateway_to_mempool) = channel::<GatewayToMempoolMessage>(1);
    let (tx_mempool_to_gateway, rx_mempool_to_gateway) = channel::<MempoolToGatewayMessage>(1);

    let gateway_network =
        GatewayNetworkComponent::new(tx_gateway_to_mempool, rx_mempool_to_gateway);
    let mut mempool_network =
        MempoolNetworkComponent::new(tx_mempool_to_gateway, rx_gateway_to_mempool);

    let tx_hash = TransactionHash::default();
    let mempool_input = MempoolInput::default();
    task::spawn(async move {
        let gateway_to_mempool = GatewayToMempoolMessage::AddTransaction(mempool_input);
        gateway_network.send(gateway_to_mempool).await.unwrap();
    })
    .await
    .unwrap();

    let mempool_message =
        task::spawn(async move { mempool_network.recv().await }).await.unwrap().unwrap();

    match mempool_message {
        GatewayToMempoolMessage::AddTransaction(mempool_input) => {
            assert_eq!(mempool_input.tx.tx_hash, tx_hash);
        }
    }
}

fn initialize_gateway_network_channels() -> (GatewayNetworkComponent, MempoolNetworkComponent) {
    let (tx_gateway_to_mempool, rx_gateway_to_mempool) = channel::<GatewayToMempoolMessage>(1);
    let (tx_mempool_to_gateway, rx_mempool_to_gateway) = channel::<MempoolToGatewayMessage>(1);

    (
        GatewayNetworkComponent::new(tx_gateway_to_mempool, rx_mempool_to_gateway),
        MempoolNetworkComponent::new(tx_mempool_to_gateway, rx_gateway_to_mempool),
    )
}

async fn set_up_gateway(network_component: GatewayNetworkComponent) -> SocketAddr {
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let port = 3000;
    let network_config = GatewayNetworkConfig { ip, port };
    let stateless_transaction_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };
    let stateful_transaction_validator_config =
        StatefulTransactionValidatorConfig::create_for_testing();
    let config = GatewayConfig {
        network_config,
        stateless_transaction_validator_config,
        stateful_transaction_validator_config,
    };

    let state_reader_factory = Arc::new(test_state_reader_factory());

    let gateway = Gateway::new(config, network_component, state_reader_factory);

    // Setup server
    tokio::spawn(async move { gateway.run_server().await });

    // TODO: Avoid using sleep, it slow down the test.
    // Ensure the server has time to start up
    sleep(Duration::from_millis(1000)).await;
    SocketAddr::from((ip, port))
}

#[rstest]
#[tokio::test]
async fn test_end_to_end() {
    let (gateway_to_mempool_network, mempool_to_gateway_network) =
        initialize_gateway_network_channels();

    let (tx_batcher_to_mempool, rx_batcher_to_mempool) = channel::<BatcherToMempoolMessage>(1);
    let (tx_mempool_to_batcher, mut rx_mempool_to_batcher) = channel::<MempoolToBatcherMessage>(1);

    let batcher_channels =
        BatcherToMempoolChannels { rx: rx_batcher_to_mempool, tx: tx_mempool_to_batcher };

    // Initialize Gateway.
    let socket_addr = set_up_gateway(gateway_to_mempool_network).await;

    // Send a transaction.
    let external_tx = invoke_tx();
    let gateway_client = gateway_client::GatewayClient::new(socket_addr);
    gateway_client.assert_add_tx_success(&external_tx, "INVOKE").await;

    // Initialize Mempool.
    let mut mempool = Mempool::empty(mempool_to_gateway_network, batcher_channels);

    task::spawn(async move {
        mempool.run().await.unwrap();
    });

    // TODO: Avoid using sleep, it slow down the test.
    // Wait for the listener to receive the transactions.
    sleep(Duration::from_secs(2)).await;

    let batcher_to_mempool_message = BatcherToMempoolMessage::GetTransactions(2);
    tx_batcher_to_mempool.send(batcher_to_mempool_message).await.unwrap();

    let mempool_message = rx_mempool_to_batcher.recv().await.unwrap();
    assert_eq!(mempool_message.len(), 1);
    assert_eq!(mempool_message[0].tip, Tip(0));
}
