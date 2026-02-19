use std::sync::Arc;
use std::time::Duration;

use apollo_config_manager_types::communication::MockConfigManagerClient;
use apollo_deployment_mode::DeploymentMode;
use apollo_mempool_config::config::{MempoolConfig, MempoolStaticConfig};
use apollo_mempool_p2p_types::communication::MockMempoolP2pPropagatorClient;
use apollo_mempool_types::communication::AddTransactionArgsWrapper;
use apollo_time::test_utils::FakeClock;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use reqwest::Url;
use rstest::rstest;
use serde::Deserialize;
use starknet_api::block::UnixTimestamp;
use tokio::net::TcpListener;

use crate::add_tx_input;
use crate::communication::MempoolCommunicationWrapper;
use crate::mempool::Mempool;

#[derive(Deserialize)]
struct TimestampQuery {
    tx_hash: String,
}

async fn start_mock_recorder(
    response: Result<UnixTimestamp, StatusCode>,
    delay: Option<Duration>,
) -> String {
    let app = Router::new()
        .route(
            "/echonet/get_timestamp",
            get(
                move |Query(query): Query<TimestampQuery>,
                      State(state): State<(
                    Result<UnixTimestamp, StatusCode>,
                    Option<Duration>,
                )>| async move {
                    // Verify tx_hash query parameter is present
                    if query.tx_hash.is_empty() {
                        return (StatusCode::BAD_REQUEST, Json(0u64)).into_response();
                    }

                    if let Some(d) = state.1 {
                        tokio::time::sleep(d).await;
                    }
                    match state.0 {
                        Ok(timestamp) => (StatusCode::OK, Json(timestamp)).into_response(),
                        Err(status) => status.into_response(),
                    }
                },
            ),
        )
        .with_state((response, delay));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("http://{}", addr)
}

#[rstest]
#[tokio::test]
async fn test_fetch_timestamps_from_recorder_success() {
    let recorder_url = start_mock_recorder(Ok(1000u64), None).await;

    let config = MempoolConfig {
        static_config: MempoolStaticConfig {
            deployment_mode: DeploymentMode::Echonet,
            recorder_url: recorder_url.parse::<Url>().unwrap(),
            ..Default::default()
        },
        ..Default::default()
    };
    let mempool = Mempool::new(config, Arc::new(FakeClock::default()));

    let mut mock_p2p = MockMempoolP2pPropagatorClient::new();
    mock_p2p.expect_add_transaction().returning(|_| Ok(()));

    let mock_config_manager = MockConfigManagerClient::new();
    let mut wrapper = MempoolCommunicationWrapper::new(
        mempool,
        Arc::new(mock_p2p),
        Arc::new(mock_config_manager),
    );

    let tx_args = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let args_wrapper = AddTransactionArgsWrapper { args: tx_args, p2p_message_metadata: None };

    let result = wrapper.add_tx(args_wrapper).await;
    assert!(result.is_ok(), "add_tx should succeed and fetch timestamps from recorder");
}
