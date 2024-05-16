use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use axum::body::{Bytes, HttpBody};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use pretty_assertions::assert_str_eq;
use rstest::{fixture, rstest};
use starknet_api::external_transaction::ExternalTransaction;
use starknet_mempool_types::mempool_types::{
    GatewayNetworkComponent, GatewayToMempoolMessage, MempoolToGatewayMessage,
};
use tokio::sync::mpsc::channel;

use crate::config::StatelessTransactionValidatorConfig;
use crate::gateway::{async_add_tx, AppState};
use crate::stateless_transaction_validator::StatelessTransactionValidator;

const TEST_FILES_FOLDER: &str = "./tests/fixtures";

#[fixture]
pub fn network_component() -> GatewayNetworkComponent {
    let (tx_gateway_to_mempool, _rx_gateway_to_mempool) = channel::<GatewayToMempoolMessage>(1);
    let (_, rx_mempool_to_gateway) = channel::<MempoolToGatewayMessage>(1);

    GatewayNetworkComponent::new(tx_gateway_to_mempool, rx_mempool_to_gateway)
}

// TODO(Ayelet): Replace the use of the JSON files with generated instances, then serialize these
// into JSON for testing.
#[rstest]
#[case::declare(&Path::new(TEST_FILES_FOLDER).join("declare_v3.json"), "DECLARE")]
#[case::deploy_account(
    &Path::new(TEST_FILES_FOLDER).join("deploy_account_v3.json"),
    "DEPLOY_ACCOUNT"
)]
#[case::invoke(&Path::new(TEST_FILES_FOLDER).join("invoke_v3.json"), "INVOKE")]
#[tokio::test]
async fn test_add_tx(
    #[case] json_file_path: &Path,
    #[case] expected_response: &str,
    network_component: GatewayNetworkComponent,
) {
    let json_file = File::open(json_file_path).unwrap();
    let tx: ExternalTransaction = serde_json::from_reader(json_file).unwrap();

    let mut app_state = AppState {
        stateless_transaction_validator: StatelessTransactionValidator {
            config: StatelessTransactionValidatorConfig {
                validate_non_zero_l1_gas_fee: true,
                max_calldata_length: 10,
                ..Default::default()
            },
        },
        network_component: Arc::new(network_component),
    };

    // Negative flow.
    const TOO_SMALL_SIGNATURE_LENGTH: usize = 0;
    app_state.stateless_transaction_validator.config.max_signature_length =
        TOO_SMALL_SIGNATURE_LENGTH;

    let response = async_add_tx(State(app_state.clone()), tx.clone().into()).await.into_response();

    let status_code = response.status();
    assert_eq!(status_code, StatusCode::INTERNAL_SERVER_ERROR);

    let response_bytes = &to_bytes(response).await;
    let negative_flow_expected_response = "Signature length exceeded maximum:";
    assert!(String::from_utf8_lossy(response_bytes).starts_with(negative_flow_expected_response));

    // Positive flow.
    app_state.stateless_transaction_validator.config.max_signature_length = 2;

    let response = async_add_tx(State(app_state), tx.into()).await.into_response();

    let status_code = response.status();
    assert_eq!(status_code, StatusCode::OK);

    let response_bytes = &to_bytes(response).await;
    assert_str_eq!(&String::from_utf8_lossy(response_bytes), expected_response);
}

async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}
