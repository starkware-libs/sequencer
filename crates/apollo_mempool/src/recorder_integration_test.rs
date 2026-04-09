use std::sync::Arc;

use apollo_config::behavior_mode::BehaviorMode;
use apollo_config_manager_types::communication::MockConfigManagerClient;
use apollo_mempool_config::config::{MempoolConfig, MempoolStaticConfig};
use apollo_mempool_p2p_types::communication::MockMempoolP2pPropagatorClient;
use apollo_mempool_types::communication::AddTransactionArgsWrapper;
use apollo_mempool_types::mempool_types::TxBlockMetadata;
use apollo_time::test_utils::FakeClock;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use reqwest::Url;
use rstest::rstest;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::transaction::TransactionHash;
use tokio::net::TcpListener;

use crate::add_tx_input;
use crate::communication::{BlockProposalParams, MempoolCommunicationWrapper};
use crate::mempool::Mempool;

// Starts a mock HTTP server simulating the recorder's get_tx_block_metadata endpoint.
// Returns the base URL (e.g., "http://127.0.0.1:12345").
async fn mock_tx_metadata_recorder(
    tx_metadata_response: Result<TxBlockMetadata, StatusCode>,
) -> String {
    mock_recorder(tx_metadata_response, Err(StatusCode::NOT_FOUND)).await
}

// Starts a mock HTTP server simulating both recorder endpoints.
// Returns the base URL (e.g., "http://127.0.0.1:12345").
async fn mock_recorder(
    tx_metadata_response: Result<TxBlockMetadata, StatusCode>,
    block_metadata_response: Result<BlockProposalParams, StatusCode>,
) -> String {
    let app = Router::new()
        .route(
            "/echonet/get_tx_block_metadata",
            get(move || async move {
                match tx_metadata_response {
                    Ok(metadata) => (StatusCode::OK, Json(metadata)).into_response(),
                    Err(status) => status.into_response(),
                }
            }),
        )
        .route(
            "/echonet/get_block_metadata",
            get(move || async move {
                match block_metadata_response {
                    Ok(metadata) => (StatusCode::OK, Json(metadata)).into_response(),
                    Err(status) => status.into_response(),
                }
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("http://{}", addr)
}

fn create_mempool_communication_wrapper(recorder_url: String) -> MempoolCommunicationWrapper {
    let config = MempoolConfig {
        static_config: MempoolStaticConfig {
            behavior_mode: BehaviorMode::Echonet,
            recorder_url: recorder_url.parse::<Url>().unwrap(),
            ..Default::default()
        },
        ..Default::default()
    };
    let mempool = Mempool::new(config, Arc::new(FakeClock::default()));

    let mut mock_p2p = MockMempoolP2pPropagatorClient::new();
    mock_p2p.expect_add_transaction().returning(|_| Ok(()));

    let mock_config_manager = MockConfigManagerClient::new();

    MempoolCommunicationWrapper::new(mempool, Arc::new(mock_p2p), Arc::new(mock_config_manager))
}

#[rstest]
#[tokio::test]
async fn test_fetch_tx_block_metadata_success() {
    let recorder_url = mock_tx_metadata_recorder(Ok(TxBlockMetadata {
        timestamp: 1000,
        block_number: BlockNumber(1234),
    }))
    .await;
    let mut wrapper = create_mempool_communication_wrapper(recorder_url);

    let tx_hash = TransactionHash::default();
    let result = wrapper.fetch_and_update_tx_block_metadata(tx_hash).await;

    assert!(result, "Should return true when recorder returns valid tx block metadata");
}

#[rstest]
#[tokio::test]
async fn test_fetch_tx_block_metadata_fails_on_http_error() {
    let recorder_url = mock_tx_metadata_recorder(Err(StatusCode::INTERNAL_SERVER_ERROR)).await;
    let mut wrapper = create_mempool_communication_wrapper(recorder_url);

    let tx_hash = TransactionHash::default();
    let result = wrapper.fetch_and_update_tx_block_metadata(tx_hash).await;

    assert!(!result, "Should return false when recorder returns HTTP error");
}

// Integration test: verifies add_tx with recorder doesn't hang or panic.
#[rstest]
#[tokio::test]
async fn test_add_tx_with_recorder_integration() {
    let recorder_url = mock_tx_metadata_recorder(Ok(TxBlockMetadata {
        timestamp: 1000,
        block_number: BlockNumber(1234),
    }))
    .await;
    let mut wrapper = create_mempool_communication_wrapper(recorder_url);

    let tx_args = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let args_wrapper = AddTransactionArgsWrapper { args: tx_args, p2p_message_metadata: None };

    wrapper.add_tx(args_wrapper).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn test_resolve_block_metadata_echonet_success() {
    // Mempool provides timestamp and block_number via tx_metadata.
    let timestamp = 1000;
    let block_number = BlockNumber(1234);
    let tx_metadata = TxBlockMetadata { timestamp, block_number };

    // Echonet provides gas prices.
    let l1_gas_price_wei = GasPrice(100);
    let l1_data_gas_price_wei = GasPrice(200);
    let l1_gas_price_fri = GasPrice(300);
    let l1_data_gas_price_fri = GasPrice(400);
    let block_metadata = BlockProposalParams {
        l1_gas_price_wei,
        l1_data_gas_price_wei,
        l1_gas_price_fri,
        l1_data_gas_price_fri,
    };

    let recorder_url = mock_recorder(Ok(tx_metadata), Ok(block_metadata)).await;
    let mut wrapper = create_mempool_communication_wrapper(recorder_url);

    let tx_args = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let args_wrapper = AddTransactionArgsWrapper { args: tx_args, p2p_message_metadata: None };
    wrapper.add_tx(args_wrapper).await.unwrap();

    let result = wrapper.resolve_block_metadata().await.unwrap();

    // timestamp and block_number come from mempool, not echonet.
    assert_eq!(result.timestamp, timestamp);
    assert_eq!(result.block_number, Some(block_number));
    // Gas prices come from echonet.
    assert_eq!(result.l1_gas_price_wei, l1_gas_price_wei);
    assert_eq!(result.l1_data_gas_price_wei, l1_data_gas_price_wei);
    assert_eq!(result.l1_gas_price_fri, l1_gas_price_fri);
    assert_eq!(result.l1_data_gas_price_fri, l1_data_gas_price_fri);
}

#[rstest]
#[tokio::test]
async fn test_resolve_block_metadata_echonet_falls_back_on_http_error() {
    let tx_metadata = TxBlockMetadata { timestamp: 1000, block_number: BlockNumber(1234) };

    let recorder_url = mock_recorder(Ok(tx_metadata), Err(StatusCode::INTERNAL_SERVER_ERROR)).await;
    let mut wrapper = create_mempool_communication_wrapper(recorder_url);

    let tx_args = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let args_wrapper = AddTransactionArgsWrapper { args: tx_args, p2p_message_metadata: None };
    wrapper.add_tx(args_wrapper).await.unwrap();

    let result = wrapper.resolve_block_metadata().await.unwrap();

    assert_eq!(result.l1_gas_price_wei, GasPrice::default());
    assert_eq!(result.l1_data_gas_price_wei, GasPrice::default());
    assert_eq!(result.l1_gas_price_fri, GasPrice::default());
    assert_eq!(result.l1_data_gas_price_fri, GasPrice::default());
}
