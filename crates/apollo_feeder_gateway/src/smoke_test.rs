use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use apollo_feeder_gateway_config::config::{FeederGatewayConfig, FeederGatewayContractAddresses};
use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_storage::header::HeaderStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::core::{EthAddress, SequencerPublicKey};
use starknet_api::crypto::utils::{PublicKey, Signature};
use starknet_api::hash::StarkHash;

use crate::feeder_gateway::FeederGateway;
use crate::reader::colocated::ColocatedStorageReader;
use crate::reader::executor::ReadExecutor;

/// Polls `is_alive` until the spawned server accepts connections (the bind is asynchronous).
async fn wait_until_alive(client: &reqwest::Client, base_url: &str) {
    for _ in 0..50 {
        if let Ok(response) = client.get(format!("{base_url}/is_alive")).send().await {
            if response.status().is_success() {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("feeder gateway server did not become alive");
}

/// Boots the real server (`FeederGateway::run`) over real test storage through the colocated
/// backend and exercises every served route over HTTP, asserting byte-exact success bodies and
/// legacy error envelopes end-to-end.
#[tokio::test(flavor = "multi_thread")]
async fn all_routes_serve_byte_parity_responses_end_to_end() {
    let ((storage_reader, mut writer), _temp_dir) = get_test_storage();
    let header =
        BlockHeader { block_hash: BlockHash(StarkHash::from(0xabc_u128)), ..Default::default() };
    let signature =
        BlockSignature(Signature { r: StarkHash::from(0x2_u128), s: StarkHash::from(0x3_u128) });
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &header)
        .unwrap()
        .append_block_signature(BlockNumber(0), &signature)
        .unwrap()
        .commit()
        .unwrap();

    let port =
        AvailablePorts::new(TestIdentifier::FeederGatewayUnitTests.into(), 0).get_next_port();
    let config = FeederGatewayConfig {
        ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port,
        contract_addresses: FeederGatewayContractAddresses {
            l1_contract_addresses: vec![(
                "Starknet".to_string(),
                EthAddress::try_from(
                    StarkHash::from_hex("0xc662c410c0ecf747543f5ba90660f6abebd9c8c4").unwrap(),
                )
                .unwrap(),
            )],
            ..Default::default()
        },
        sequencer_public_key: SequencerPublicKey(PublicKey(StarkHash::from(0x1252_u128))),
        ..Default::default()
    };
    let chain_data_reader =
        Arc::new(ColocatedStorageReader::new(storage_reader, Arc::new(ReadExecutor::new(2))));
    let mut feeder_gateway = FeederGateway::new(config, chain_data_reader);
    tokio::spawn(async move { feeder_gateway.run().await.unwrap() });

    let base_url = format!("http://127.0.0.1:{port}/feeder_gateway");
    let client = reqwest::Client::new();
    wait_until_alive(&client, &base_url).await;

    let route_expectations = [
        ("is_ready", 200, "FeederGateway is ready"),
        (
            "get_contract_addresses",
            200,
            r#"{"Starknet": "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4", "strk_l2_token_address": "0x0", "eth_l2_token_address": "0x0"}"#,
        ),
        ("get_public_key", 200, r#""0x1252""#),
        (
            "get_signature?blockNumber=0",
            200,
            r#"{"block_hash": "0xabc", "signature": ["0x2", "0x3"]}"#,
        ),
        ("get_block_hash_by_id?blockId=0", 200, r#""0xabc""#),
        ("get_block_id_by_hash?blockHash=0xabc", 200, "0"),
        // Legacy error envelopes (byte-exact, verified against errors_test pins).
        (
            "get_signature?blockNumber=7",
            400,
            r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 7 was not found."}"#,
        ),
        (
            "get_block_hash_by_id?blockId=7",
            400,
            r#"{"code": "StarkErrorCode.MALFORMED_REQUEST", "message": "Block ID should be in the range [0, 1); got: 7."}"#,
        ),
        (
            "get_block_hash_by_id?blockId=zzz",
            400,
            r#"{"code": "StarkErrorCode.MALFORMED_REQUEST", "message": "Expecting value: line 1 column 1 (char 0)"}"#,
        ),
        (
            "get_block_id_by_hash?blockHash=0xdeadbeef",
            400,
            r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block hash 0xdeadbeef does not exist."}"#,
        ),
        (
            "get_block_id_by_hash",
            400,
            r#"{"code": "StarkErrorCode.MALFORMED_REQUEST", "message": "Block hash must be given."}"#,
        ),
    ];
    for (route, expected_status, expected_body) in route_expectations {
        let response = client.get(format!("{base_url}/{route}")).send().await.unwrap();
        assert_eq!(response.status().as_u16(), expected_status, "unexpected status for {route}");
        assert_eq!(response.text().await.unwrap(), expected_body, "unexpected body for {route}");
    }
}
