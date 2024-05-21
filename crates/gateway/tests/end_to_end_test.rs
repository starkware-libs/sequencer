use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use axum::body::{Body, HttpBody};
use axum::http::{Request, StatusCode};
use hyper::{Client, Response};
use mempool_infra::network_component::CommunicationInterface;
use rstest::rstest;
use starknet_api::transaction::{Tip, TransactionHash};
use starknet_gateway::config::{GatewayNetworkConfig, StatelessTransactionValidatorConfig};
use starknet_gateway::gateway::Gateway;
use starknet_mempool::mempool::Mempool;
use starknet_mempool_types::mempool_types::{
    BatcherToMempoolChannels, BatcherToMempoolMessage, GatewayNetworkComponent,
    GatewayToMempoolMessage, MempoolInput, MempoolNetworkComponent, MempoolToBatcherMessage,
    MempoolToGatewayMessage,
};
use tokio::sync::mpsc::channel;
use tokio::task;
use tokio::time::sleep;

const TEST_FILES_FOLDER: &str = "./tests/fixtures";

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

async fn set_up_gateway(network_component: GatewayNetworkComponent) -> (IpAddr, u16) {
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let port = 3000;
    let network_config = GatewayNetworkConfig { ip, port };
    let stateless_transaction_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };

    let gateway =
        Gateway { network_config, network_component, stateless_transaction_validator_config };

    // Setup server
    tokio::spawn(async move { gateway.build_server().await });

    // Ensure the server has time to start up
    sleep(Duration::from_millis(1000)).await;
    (ip, port)
}

async fn send_and_verify_transaction(
    ip: IpAddr,
    port: u16,
    json_file_path: &Path,
    expected_response: &str,
) {
    let tx_json = fs::read_to_string(json_file_path).unwrap();
    let request = Request::builder()
        .method("POST")
        .uri(format!("http://{}", SocketAddr::from((ip, port))) + "/add_tx")
        .header("content-type", "application/json")
        .body(Body::from(tx_json))
        .unwrap();

    // Create a client
    let client = Client::new();

    // Send a POST request with the transaction data as the body
    let response: Response<Body> = client.request(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let res = response.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(res, expected_response.as_bytes());
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
    let (ip, port) = set_up_gateway(gateway_to_mempool_network).await;

    // Send a transaction.
    let invoke_json = &Path::new(TEST_FILES_FOLDER).join("invoke_v3.json");
    send_and_verify_transaction(ip, port, invoke_json, "INVOKE").await;

    // Initialize Mempool.
    let mut mempool = Mempool::empty(mempool_to_gateway_network, batcher_channels);

    task::spawn(async move {
        mempool.run().await.unwrap();
    });

    // Wait for the listener to receive the transactions.
    sleep(Duration::from_secs(2)).await;

    let batcher_to_mempool_message = BatcherToMempoolMessage::GetTransactions(2);
    task::spawn(async move {
        tx_batcher_to_mempool.send(batcher_to_mempool_message).await.unwrap();
    });

    let mempool_message = rx_mempool_to_batcher.recv().await.unwrap();
    assert_eq!(mempool_message.len(), 1);
    assert_eq!(mempool_message[0].tip, Tip(0));
}
