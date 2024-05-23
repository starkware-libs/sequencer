use std::sync::Arc;

use axum::body::{Bytes, HttpBody};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use pretty_assertions::assert_str_eq;
use rstest::rstest;
use starknet_api::external_transaction::ExternalTransaction;
use starknet_mempool_types::mempool_types::{
    GatewayNetworkComponent, GatewayToMempoolMessage, MempoolToGatewayMessage,
};
use tokio::sync::mpsc::channel;

use crate::config::{StatefulTransactionValidatorConfig, StatelessTransactionValidatorConfig};
use crate::gateway::{add_tx, AppState};
use crate::starknet_api_test_utils::{external_invoke_tx_to_json, invoke_tx};
use crate::state_reader_test_utils::test_state_reader_factory;
use crate::stateful_transaction_validator::StatefulTransactionValidator;
use crate::stateless_transaction_validator::StatelessTransactionValidator;

// TODO(Ayelet): add test cases for declare and deploy account transactions.
#[rstest]
#[case::invoke(invoke_tx(), "INVOKE")]
#[tokio::test]
async fn test_add_tx(
    #[case] external_invoke_tx: ExternalTransaction,
    #[case] expected_response: &str,
) {
    // The  `_rx_gateway_to_mempool`   is retained to keep the channel open, as dropping it would
    // prevent the sender from transmitting messages.
    let (tx_gateway_to_mempool, _rx_gateway_to_mempool) = channel::<GatewayToMempoolMessage>(1);
    let (_, rx_mempool_to_gateway) = channel::<MempoolToGatewayMessage>(1);

    // TODO: Add fixture.
    let network_component =
        Arc::new(GatewayNetworkComponent::new(tx_gateway_to_mempool, rx_mempool_to_gateway));

    let json_string = external_invoke_tx_to_json(external_invoke_tx);
    let tx: ExternalTransaction = serde_json::from_str(&json_string).unwrap();

    let mut app_state = AppState {
        stateless_transaction_validator: StatelessTransactionValidator {
            config: StatelessTransactionValidatorConfig {
                validate_non_zero_l1_gas_fee: true,
                max_calldata_length: 10,
                ..Default::default()
            },
        },
        network_component,
        stateful_transaction_validator: Arc::new(StatefulTransactionValidator {
            config: StatefulTransactionValidatorConfig::create_for_testing(),
        }),
        state_reader_factory: Arc::new(test_state_reader_factory()),
    };

    // Negative flow.
    const TOO_SMALL_SIGNATURE_LENGTH: usize = 0;
    app_state.stateless_transaction_validator.config.max_signature_length =
        TOO_SMALL_SIGNATURE_LENGTH;

    let response = add_tx(State(app_state.clone()), tx.clone().into()).await.into_response();

    let status_code = response.status();
    assert_eq!(status_code, StatusCode::INTERNAL_SERVER_ERROR);

    let response_bytes = &to_bytes(response).await;
    let negative_flow_expected_response = "Signature length exceeded maximum:";
    assert!(String::from_utf8_lossy(response_bytes).starts_with(negative_flow_expected_response));

    // Positive flow.
    app_state.stateless_transaction_validator.config.max_signature_length = 2;

    let response = add_tx(State(app_state), tx.into()).await.into_response();

    let status_code = response.status();
    assert_eq!(status_code, StatusCode::OK);

    let response_bytes = &to_bytes(response).await;
    assert_str_eq!(&String::from_utf8_lossy(response_bytes), expected_response);
}

async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}
