use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use rstest::rstest;
use starknet_api::transaction::Tip;
use starknet_gateway::config::{
    GatewayConfig, GatewayNetworkConfig, StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway::gateway::Gateway;
use starknet_gateway::starknet_api_test_utils::invoke_tx;
use starknet_gateway::state_reader_test_utils::rpc_test_state_reader_factory;
use starknet_mempool::communication::create_mempool_server;
use starknet_mempool::mempool::Mempool;
use starknet_mempool_integration_tests::integration_test_utils::GatewayClient;
use starknet_mempool_types::communication::{
    MempoolClient, MempoolClientImpl, MempoolRequestAndResponseSender, SharedMempoolClient,
};
use tokio::sync::mpsc::channel;
use tokio::task;
use tokio::time::sleep;

const MEMPOOL_INVOCATIONS_QUEUE_SIZE: usize = 32;

async fn set_up_gateway(mempool_client: SharedMempoolClient) -> SocketAddr {
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

    let state_reader_factory = Arc::new(rpc_test_state_reader_factory().await);

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
    // Initialize Mempool.
    // TODO(Tsabary): wrap creation of channels in dedicated functions, take channel capacity from
    // config.
    let (tx_mempool, rx_mempool) =
        channel::<MempoolRequestAndResponseSender>(MEMPOOL_INVOCATIONS_QUEUE_SIZE);
    let mempool = Mempool::empty();
    let mut mempool_server = create_mempool_server(mempool, rx_mempool);

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
