use std::sync::Arc;

use apollo_feeder_gateway_config::config::{FeederGatewayConfig, FeederGatewayContractAddresses};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use starknet_api::block::{BlockHash, BlockNumber, BlockSignature};
use starknet_api::core::{ContractAddress, SequencerPublicKey};
use starknet_api::crypto::utils::{PublicKey, Signature};
use starknet_api::hash::StarkHash;
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

#[tokio::test]
async fn get_public_key_returns_bare_felt() {
    let config = FeederGatewayConfig {
        sequencer_public_key: SequencerPublicKey(PublicKey(StarkHash::from(0x1252_u128))),
        ..Default::default()
    };
    let feeder_gateway = FeederGateway::new(config, Arc::new(MockChainDataReader::new()));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder().uri("/feeder_gateway/get_public_key").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    // The live feeder gateway serves the public key as a bare quoted felt (e.g. "0x1252b6...").
    assert_eq!(&body[..], br#""0x1252""#);
}

#[tokio::test]
async fn get_signature_returns_byte_parity_json() {
    let mut reader = MockChainDataReader::new();
    reader.expect_block_signature().returning(|_| {
        Ok((
            BlockHash(StarkHash::from(0x1_u128)),
            BlockSignature(Signature {
                r: StarkHash::from(0x2_u128),
                s: StarkHash::from(0x3_u128),
            }),
        ))
    });
    let feeder_gateway = FeederGateway::new(FeederGatewayConfig::default(), Arc::new(reader));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_signature?blockNumber=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    // Matches the live shape: {"block_hash": "0x..", "signature": ["0x..", "0x.."]}.
    assert_eq!(&body[..], br#"{"block_hash": "0x1", "signature": ["0x2", "0x3"]}"#);
}

#[tokio::test]
async fn get_block_hash_by_id_returns_byte_parity_hash() {
    let mut reader = MockChainDataReader::new();
    reader.expect_block_hash().returning(|_| Ok(BlockHash(StarkHash::from(0x1234_u128))));
    let feeder_gateway = FeederGateway::new(FeederGatewayConfig::default(), Arc::new(reader));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_block_hash_by_id?blockId=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], br#""0x1234""#);
}

#[tokio::test]
async fn get_block_id_by_hash_returns_byte_parity_number() {
    let mut reader = MockChainDataReader::new();
    reader.expect_block_number_by_hash().returning(|_| Ok(Some(BlockNumber(1))));
    let feeder_gateway = FeederGateway::new(FeederGatewayConfig::default(), Arc::new(reader));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_block_id_by_hash?blockHash=0x1234")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    // The live feeder gateway serves the block number as a bare unquoted number.
    assert_eq!(&body[..], br#"1"#);
}

#[tokio::test]
async fn get_block_id_by_hash_unknown_hash_is_block_not_found() {
    let mut reader = MockChainDataReader::new();
    reader.expect_block_number_by_hash().returning(|_| Ok(None));
    let feeder_gateway = FeederGateway::new(FeederGatewayConfig::default(), Arc::new(reader));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_block_id_by_hash?blockHash=0x1234")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Verified live: an unknown hash is the BLOCK_NOT_FOUND envelope with HTTP 400.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8(body.to_vec()).unwrap().contains("BLOCK_NOT_FOUND"));
}

#[tokio::test]
async fn get_block_id_by_hash_missing_param_is_bad_request() {
    let feeder_gateway =
        FeederGateway::new(FeederGatewayConfig::default(), Arc::new(MockChainDataReader::new()));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_block_id_by_hash")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_block_id_by_hash_without_0x_prefix_is_bad_request() {
    let feeder_gateway =
        FeederGateway::new(FeederGatewayConfig::default(), Arc::new(MockChainDataReader::new()));

    // Verified live: bare-hex and 0X-prefixed forms are rejected as MALFORMED_REQUEST.
    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_block_id_by_hash?blockHash=1234")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_block_id_by_hash_non_hex_is_bad_request() {
    let feeder_gateway =
        FeederGateway::new(FeederGatewayConfig::default(), Arc::new(MockChainDataReader::new()));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_block_id_by_hash?blockHash=0xzzz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_block_hash_by_id_missing_param_is_bad_request() {
    let feeder_gateway =
        FeederGateway::new(FeederGatewayConfig::default(), Arc::new(MockChainDataReader::new()));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_block_hash_by_id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
