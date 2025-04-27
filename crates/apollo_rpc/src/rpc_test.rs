use std::panic;

use apollo_storage::base_layer::BaseLayerStorageWriter;
use apollo_storage::header::HeaderStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use assert_matches::assert_matches;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::{Error, RpcResult};
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::types::ErrorObjectOwned;
use pretty_assertions::assert_eq;
use starknet_api::block::{
    BlockHash,
    BlockHeader,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockStatus,
};

use crate::test_utils::{
    get_test_highest_block,
    get_test_pending_classes,
    get_test_pending_data,
    get_test_rpc_config,
};
use crate::{get_block_status, run_server};

#[tokio::test]
async fn run_server_no_blocks() {
    let ((storage_reader, _), _temp_dir) = get_test_storage();
    let gateway_config = get_test_rpc_config();
    let shared_highest_block = get_test_highest_block();
    let pending_data = get_test_pending_data();
    let pending_classes = get_test_pending_classes();
    let (addr, _handle) = run_server(
        &gateway_config,
        shared_highest_block,
        pending_data,
        pending_classes,
        storage_reader,
        "NODE VERSION",
        None,
    )
    .await
    .unwrap();
    let client = HttpClientBuilder::default().build(format!("http://{addr:?}")).unwrap();
    let res: Result<RpcResult<BlockNumber>, Error> =
        client.request("starknet_blockNumber", [""]).await;
    let _expected_error = ErrorObjectOwned::owned(123, "", None::<u8>);
    // TODO(yair): fix this test:
    // 1. assert_matches doesn't compare the values, just the types
    // 2. the error is not the expected one
    // 3. the expected error should be "Invalid path format: {path}"
    match res {
        Err(err) => assert_matches!(err, _expected_error),
        Ok(_) => panic!("should error with no blocks"),
    };
}

#[test]
fn get_block_status_test() {
    let (reader, mut writer) = get_test_storage().0;

    for block_number in 0..2 {
        let header = BlockHeader {
            block_hash: BlockHash(block_number.into()),
            block_header_without_hash: BlockHeaderWithoutHash {
                block_number: BlockNumber(block_number),
                ..Default::default()
            },
            ..Default::default()
        };
        writer
            .begin_rw_txn()
            .unwrap()
            .append_header(header.block_header_without_hash.block_number, &header)
            .unwrap()
            .commit()
            .unwrap();
    }

    // update the base_layer_tip_marker to BlockNumber(1).
    writer
        .begin_rw_txn()
        .unwrap()
        .update_base_layer_block_marker(&BlockNumber(1))
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    assert_eq!(get_block_status(&txn, BlockNumber(0)).unwrap(), BlockStatus::AcceptedOnL1);
    assert_eq!(get_block_status(&txn, BlockNumber(1)).unwrap(), BlockStatus::AcceptedOnL2);
    assert_eq!(get_block_status(&txn, BlockNumber(2)).unwrap(), BlockStatus::AcceptedOnL2);
}

#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}
