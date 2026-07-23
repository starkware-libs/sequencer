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
use starknet_api::block::{BlockNumber, GasPrice, ReplayBlockMetadata};
use starknet_api::transaction::TransactionHash;
use tokio::net::TcpListener;

use crate::add_tx_input;
use crate::communication::{MempoolCommunicationWrapper, RecorderBlockMetadata};
use crate::mempool::Mempool;

// Starts a mock HTTP server that simulates the recorder's get_tx_block_metadata endpoint.
// Returns the base URL (e.g., "http://127.0.0.1:12345").
async fn start_mock_recorder(tx_metadata_response: Result<TxBlockMetadata, StatusCode>) -> String {
    start_mock_recorder_with_block_metadata(tx_metadata_response, Err(StatusCode::NOT_FOUND)).await
}

// Starts a mock HTTP server that simulates the recorder's get_tx_block_metadata and
// get_block_metadata endpoints. Returns the base URL (e.g., "http://127.0.0.1:12345").
async fn start_mock_recorder_with_block_metadata(
    tx_metadata_response: Result<TxBlockMetadata, StatusCode>,
    block_metadata_response: Result<RecorderBlockMetadata, StatusCode>,
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
    let recorder_url = start_mock_recorder(Ok(TxBlockMetadata {
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
    let recorder_url = start_mock_recorder(Err(StatusCode::INTERNAL_SERVER_ERROR)).await;
    let mut wrapper = create_mempool_communication_wrapper(recorder_url);

    let tx_hash = TransactionHash::default();
    let result = wrapper.fetch_and_update_tx_block_metadata(tx_hash).await;

    assert!(!result, "Should return false when recorder returns HTTP error");
}

// Integration test: verifies add_tx with recorder doesn't hang or panic.
#[rstest]
#[tokio::test]
async fn test_add_tx_with_recorder_integration() {
    let recorder_url = start_mock_recorder(Ok(TxBlockMetadata {
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
async fn test_resolve_block_metadata_uses_recorder_timestamp_and_gas_prices() {
    // The tx-derived timestamp differs from the recorder's block timestamp; the recorder's
    // value is authoritative.
    let tx_metadata = TxBlockMetadata { timestamp: 1000, block_number: BlockNumber(1234) };
    let recorder_metadata = RecorderBlockMetadata {
        timestamp: 2000,
        l1_gas_price_wei: GasPrice(100),
        l1_data_gas_price_wei: GasPrice(200),
        l1_gas_price_fri: GasPrice(300),
        l1_data_gas_price_fri: GasPrice(400),
        l2_gas_price_fri: GasPrice(500),
    };

    let recorder_url =
        start_mock_recorder_with_block_metadata(Ok(tx_metadata), Ok(recorder_metadata.clone()))
            .await;
    let mut wrapper = create_mempool_communication_wrapper(recorder_url);

    let tx_args = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    wrapper
        .add_tx(AddTransactionArgsWrapper { args: tx_args, p2p_message_metadata: None })
        .await
        .unwrap();

    let resolved_metadata = wrapper.resolve_block_metadata().await.unwrap();

    assert_eq!(
        resolved_metadata,
        ReplayBlockMetadata {
            timestamp: recorder_metadata.timestamp,
            // The block number comes from the mempool's tx metadata, not the recorder.
            block_number: Some(BlockNumber(1234)),
            l1_gas_price_wei: recorder_metadata.l1_gas_price_wei,
            l1_data_gas_price_wei: recorder_metadata.l1_data_gas_price_wei,
            l1_gas_price_fri: recorder_metadata.l1_gas_price_fri,
            l1_data_gas_price_fri: recorder_metadata.l1_data_gas_price_fri,
            l2_gas_price_fri: recorder_metadata.l2_gas_price_fri,
        }
    );
}

#[rstest]
#[tokio::test]
async fn test_resolve_block_metadata_falls_back_on_http_error() {
    let tx_metadata = TxBlockMetadata { timestamp: 1000, block_number: BlockNumber(1234) };

    let recorder_url = start_mock_recorder_with_block_metadata(
        Ok(tx_metadata),
        Err(StatusCode::INTERNAL_SERVER_ERROR),
    )
    .await;
    let mut wrapper = create_mempool_communication_wrapper(recorder_url);

    let tx_args = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    wrapper
        .add_tx(AddTransactionArgsWrapper { args: tx_args, p2p_message_metadata: None })
        .await
        .unwrap();

    let resolved_metadata = wrapper.resolve_block_metadata().await.unwrap();

    // The mempool-derived timing fields survive; gas prices fall back to zero.
    assert_eq!(
        resolved_metadata,
        ReplayBlockMetadata {
            timestamp: 1000,
            block_number: Some(BlockNumber(1234)),
            ..Default::default()
        }
    );
}
