use std::sync::Arc;

use apollo_feeder_gateway_config::config::{FeederGatewayConfig, FeederGatewayContractAddresses};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use rstest::rstest;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::core::{ContractAddress, EthAddress, SequencerPublicKey};
use starknet_api::crypto::utils::{PublicKey, Signature};
use starknet_api::hash::StarkHash;
use tower::util::ServiceExt;

use crate::errors::FeederGatewayError;
use crate::feeder_gateway::FeederGateway;
use crate::reader::MockChainDataReader;

/// Parses a (lowercased) L1 address literal for test configs.
fn l1_contract_address(address_hex: &str) -> EthAddress {
    EthAddress::try_from(StarkHash::from_hex(address_hex).unwrap()).unwrap()
}

/// Parses an L2 address literal for test configs.
fn l2_token_address(address_hex: &str) -> ContractAddress {
    ContractAddress::try_from(StarkHash::from_hex(address_hex).unwrap()).unwrap()
}

/// The L2 fee-token addresses are identical on mainnet and sepolia (verified live).
fn live_l2_token_addresses() -> (ContractAddress, ContractAddress) {
    (
        l2_token_address("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
        l2_token_address("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
    )
}

/// The live mainnet L1 contract set and order (captured 2026-06-03).
fn mainnet_contract_addresses() -> FeederGatewayContractAddresses {
    let (strk_l2_token_address, eth_l2_token_address) = live_l2_token_addresses();
    FeederGatewayContractAddresses {
        l1_contract_addresses: vec![
            (
                "Starknet".to_string(),
                l1_contract_address("0xc662c410c0ecf747543f5ba90660f6abebd9c8c4"),
            ),
            (
                "GpsStatementVerifier".to_string(),
                l1_contract_address("0x47312450b3ac8b5b8e247a6bb6d523e7605bdb60"),
            ),
        ],
        strk_l2_token_address,
        eth_l2_token_address,
    }
}

/// The live sepolia L1 contract set and order (captured 2026-06-03): a different set, in a
/// different order, than mainnet.
fn sepolia_contract_addresses() -> FeederGatewayContractAddresses {
    let (strk_l2_token_address, eth_l2_token_address) = live_l2_token_addresses();
    FeederGatewayContractAddresses {
        l1_contract_addresses: vec![
            (
                "GpsStatementVerifier".to_string(),
                l1_contract_address("0xf294781d719d2f4169ce54469c28908e6fa752c1"),
            ),
            (
                "MemoryPageFactRegistry".to_string(),
                l1_contract_address("0x5628e75245cc69eca0994f0449f4dda9fbb5ec6a"),
            ),
            (
                "MerkleStatementContract".to_string(),
                l1_contract_address("0xd414f8f535d4a96cb00ffc8e85160b353cb7809c"),
            ),
            (
                "FriStatementContract".to_string(),
                l1_contract_address("0x55d049b4c82807808e76e61a08c6764bbf2ffb55"),
            ),
            (
                "Starknet".to_string(),
                l1_contract_address("0xe2bb56ee936fd6433dc0f6e7e3b8365c906aa057"),
            ),
            (
                "HybridGpsFactAdapter".to_string(),
                l1_contract_address("0x68cb84164e27cbf65222f604baef58cc4149fcfc"),
            ),
        ],
        strk_l2_token_address,
        eth_l2_token_address,
    }
}

/// Byte-parity against the captured live responses: the configured set, order, EIP-55
/// checksumming, and trailing L2 felts must reproduce the live bytes exactly per network.
#[rstest]
#[case::mainnet(mainnet_contract_addresses(), "contract_addresses_mainnet.json")]
#[case::sepolia(sepolia_contract_addresses(), "contract_addresses_sepolia.json")]
#[tokio::test]
async fn get_contract_addresses_returns_live_byte_parity_json(
    #[case] contract_addresses: FeederGatewayContractAddresses,
    #[case] fixture_name: &str,
) {
    let fixture_path = format!("{}/resources/parity/{fixture_name}", env!("CARGO_MANIFEST_DIR"));
    let live_bytes = std::fs::read_to_string(fixture_path).unwrap();
    let config = FeederGatewayConfig { contract_addresses, ..Default::default() };
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
    assert_eq!(String::from_utf8(body.to_vec()).unwrap(), live_bytes);
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
