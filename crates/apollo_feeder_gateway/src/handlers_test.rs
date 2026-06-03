use std::sync::Arc;

use apollo_feeder_gateway_config::config::{FeederGatewayConfig, FeederGatewayContractAddresses};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use rstest::rstest;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::core::{ContractAddress, SequencerPublicKey};
use starknet_api::crypto::utils::{PublicKey, Signature};
use starknet_api::hash::StarkHash;
use tower::util::ServiceExt;

use crate::errors::FeederGatewayError;
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
async fn get_block_hash_by_id_out_of_range_is_malformed_request() {
    let mut reader = MockChainDataReader::new();
    reader
        .expect_block_hash()
        .returning(|block_number| Err(FeederGatewayError::BlockNotFound(block_number)));
    // The range bound in the message is one past the latest synced block.
    reader.expect_latest_block_header().returning(|| {
        let mut header = BlockHeader::default();
        header.block_header_without_hash.block_number = BlockNumber(5);
        Ok(Some(header))
    });
    let feeder_gateway = FeederGateway::new(FeederGatewayConfig::default(), Arc::new(reader));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_block_hash_by_id?blockId=99999999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Verified live: a block id beyond the chain head is MALFORMED_REQUEST with the range
    // message, not BLOCK_NOT_FOUND.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        String::from_utf8(body.to_vec()).unwrap(),
        r#"{"code": "StarkErrorCode.MALFORMED_REQUEST", "message": "Block ID should be in the range [0, 6); got: 99999999."}"#
    );
}

/// Live semantics: blockId goes through Python's int(json.loads(value)), so floats truncate and
/// booleans coerce (verified live: blockId=1.5 and blockId=true both serve block 1).
#[rstest]
#[case::float_truncates("1.5")]
#[case::bool_coerces("true")]
#[tokio::test]
async fn get_block_hash_by_id_coerces_to_int_like_python(#[case] block_id: &str) {
    let mut reader = MockChainDataReader::new();
    reader
        .expect_block_hash()
        .withf(|block_number| *block_number == BlockNumber(1))
        .returning(|_| Ok(BlockHash(StarkHash::from(0x1234_u128))));
    let feeder_gateway = FeederGateway::new(FeederGatewayConfig::default(), Arc::new(reader));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri(format!("/feeder_gateway/get_block_hash_by_id?blockId={block_id}"))
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
async fn get_block_hash_by_id_null_echoes_python_none_int_error() {
    let feeder_gateway =
        FeederGateway::new(FeederGatewayConfig::default(), Arc::new(MockChainDataReader::new()));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri("/feeder_gateway/get_block_hash_by_id?blockId=null")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    // The exact live message: Python's int(None) TypeError echo.
    assert_eq!(
        String::from_utf8(body.to_vec()).unwrap(),
        r#"{"code": "StarkErrorCode.MALFORMED_REQUEST", "message": "int() argument must be a string, a bytes-like object or a number, not 'NoneType'"}"#
    );
}

/// Live semantics: a missing or null blockNumber serves the LATEST synced block's signature.
#[rstest]
#[case::missing_param("")]
#[case::null_param("?blockNumber=null")]
#[tokio::test]
async fn get_signature_without_block_number_serves_latest(#[case] query_string: &str) {
    let mut reader = MockChainDataReader::new();
    reader.expect_latest_block_header().returning(|| {
        let mut header = BlockHeader::default();
        header.block_header_without_hash.block_number = BlockNumber(9);
        Ok(Some(header))
    });
    reader
        .expect_block_signature()
        .withf(|block_number| *block_number == BlockNumber(9))
        .returning(|_| {
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
                .uri(format!("/feeder_gateway/get_signature{query_string}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], br#"{"block_hash": "0x1", "signature": ["0x2", "0x3"]}"#);
}

/// Every expected message is the exact live response text for the same input.
#[rstest]
#[case::negative_int(
    "-1",
    r#"{"code": "StarkErrorCode.MALFORMED_REQUEST", "message": "Field blockNumber must be a non-negative integer, or 'null';got: int(-1)."}"#
)]
#[case::float_rejected(
    "1.5",
    r#"{"code": "StarkErrorCode.MALFORMED_REQUEST", "message": "Field blockNumber must be a non-negative integer, or 'null';got: float(1.5)."}"#
)]
#[case::beyond_u64_is_not_found(
    "99999999999999999999999",
    r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 99999999999999999999999 was not found."}"#
)]
#[tokio::test]
async fn get_signature_invalid_block_number_replicates_live_messages(
    #[case] block_number: &str,
    #[case] live_body: &str,
) {
    let feeder_gateway =
        FeederGateway::new(FeederGatewayConfig::default(), Arc::new(MockChainDataReader::new()));

    let response = feeder_gateway
        .app()
        .oneshot(
            Request::builder()
                .uri(format!("/feeder_gateway/get_signature?blockNumber={block_number}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(String::from_utf8(body.to_vec()).unwrap(), live_body);
}

#[tokio::test]
async fn get_block_hash_by_id_missing_param_echoes_python_key_error() {
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
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    // The exact live message: Python's KeyError echo for the missing field.
    assert_eq!(
        String::from_utf8(body.to_vec()).unwrap(),
        r#"{"code": "StarkErrorCode.MALFORMED_REQUEST", "message": "'blockId'"}"#
    );
}
