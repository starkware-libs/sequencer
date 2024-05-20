use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use mempool_infra::component_server::ComponentServer;
use mempool_infra::network_component::CommunicationInterface;
use rstest::rstest;
use starknet_api::transaction::{Tip, TransactionHash};
use starknet_gateway::config::{
    GatewayConfig, GatewayNetworkConfig, StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway::gateway::Gateway;
use starknet_gateway::starknet_api_test_utils::invoke_tx;
use starknet_gateway::state_reader_test_utils::test_state_reader_factory;
use starknet_mempool::mempool::{Mempool, MempoolCommunicationWrapper};
use starknet_mempool_integration_tests::integration_test_utils::GatewayClient;
use starknet_mempool_types::mempool_types::{
    BatcherToMempoolChannels, BatcherToMempoolMessage, GatewayNetworkComponent,
    GatewayToMempoolMessage, MempoolClient, MempoolClientImpl, MempoolInput,
    MempoolNetworkComponent, MempoolRequestAndResponseSender, MempoolToBatcherMessage,
    MempoolToGatewayMessage,
};
use tokio::sync::mpsc::channel;
use tokio::task;
use tokio::time::sleep;

const MEMPOOL_INVOCATIONS_QUEUE_SIZE: usize = 32;

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

async fn set_up_gateway(mempool_client: Arc<dyn MempoolClient>) -> SocketAddr {
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let port = 3000;
    let network_config = GatewayNetworkConfig { ip, port };
    let stateless_tx_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };
    let stateful_tx_validator_config = StatefulTransactionValidatorConfig::create_for_testing();
    let config = GatewayConfig {
        network_config,
        stateless_tx_validator_config,
        stateful_tx_validator_config,
    };

    let state_reader_factory = Arc::new(test_state_reader_factory());

    let gateway = Gateway::new(config, state_reader_factory, mempool_client);

    // Setup server
    tokio::spawn(async move { gateway.run().await });

    // TODO: Avoid using sleep, it slow down the test.
    // Ensure the server has time to start up
    sleep(Duration::from_millis(1000)).await;
    SocketAddr::from((ip, port))
}

#[rstest]
#[tokio::test]
async fn test_end_to_end() {
    // TODO: delete this line once deprecating network component.
    let (_, mempool_to_gateway_network) = initialize_gateway_network_channels();

    let (_tx_batcher_to_mempool, rx_batcher_to_mempool) = channel::<BatcherToMempoolMessage>(1);
    let (tx_mempool_to_batcher, _rx_mempool_to_batcher) = channel::<MempoolToBatcherMessage>(1);

    let batcher_channels =
        BatcherToMempoolChannels { rx: rx_batcher_to_mempool, tx: tx_mempool_to_batcher };

    // Initialize Mempool.
    // TODO(Tsabary): wrap creation of channels in dedicated functions, take channel capacity from
    // config.
    let (tx_mempool, rx_mempool) =
        channel::<MempoolRequestAndResponseSender>(MEMPOOL_INVOCATIONS_QUEUE_SIZE);
    let mempool = Mempool::empty(mempool_to_gateway_network, batcher_channels);

    // TODO(Tsabary, 1/6/2024): Wrap with a dedicated create_mempool_server function.
    let mut mempool_server =
        ComponentServer::new(MempoolCommunicationWrapper::new(mempool), rx_mempool);
    task::spawn(async move {
        mempool_server.start().await;
    });

    // Initialize Gateway.
    let gateway_mempool_client = Arc::new(MempoolClientImpl::new(tx_mempool.clone()));
    let socket_addr = set_up_gateway(gateway_mempool_client).await;

    // Send a transaction.
    let external_tx = invoke_tx();
    let gateway_client = GatewayClient::new(socket_addr);
    gateway_client.assert_add_tx_success(&external_tx).await;

    let batcher_mempool_client = MempoolClientImpl::new(tx_mempool.clone());
    let mempool_message = batcher_mempool_client.get_txs(2).await.unwrap();

    assert_eq!(mempool_message.len(), 1);
    assert_eq!(mempool_message[0].tip, Tip(0));
}
