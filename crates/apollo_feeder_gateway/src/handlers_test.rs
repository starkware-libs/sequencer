use std::sync::Arc;

use apollo_feeder_gateway_config::config::{FeederGatewayConfig, FeederGatewayContractAddresses};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use starknet_api::core::ContractAddress;
use tower::util::ServiceExt;

use crate::feeder_gateway::FeederGateway;
use crate::reader::MockChainDataReader;

#[tokio::test]
async fn get_contract_addresses_returns_byte_parity_json() {
    let config = FeederGatewayConfig {
        contract_addresses: FeederGatewayContractAddresses {
            starknet: ContractAddress::from(0x1234_u128),
            gps_statement_verifier: ContractAddress::from(0xabcd_u128),
        },
        ..Default::default()
    };
    let feeder_gateway = FeederGateway::new(config, Arc::new(MockChainDataReader::new()));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_contract_addresses")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], br#"{"Starknet": "0x1234", "GpsStatementVerifier": "0xabcd"}"#);
}
