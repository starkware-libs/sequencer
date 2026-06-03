use std::sync::Arc;

use apollo_storage::header::HeaderStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use starknet_api::block::{BlockHeader, BlockNumber};

use crate::errors::FeederGatewayError;
use crate::reader::colocated::ColocatedStorageReader;
use crate::reader::executor::ReadExecutor;
use crate::reader::ChainDataReader;

fn executor() -> Arc<ReadExecutor> {
    Arc::new(ReadExecutor::new(2))
}

#[tokio::test]
async fn latest_block_header_empty_storage_returns_none() {
    let ((storage_reader, _writer), _temp_dir) = get_test_storage();
    let reader = ColocatedStorageReader::new(storage_reader, executor());

    assert!(reader.latest_block_header().await.unwrap().is_none());
}

#[tokio::test]
async fn latest_block_header_returns_appended_header() {
    let ((storage_reader, mut writer), _temp_dir) = get_test_storage();
    let header = BlockHeader::default();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &header)
        .unwrap()
        .commit()
        .unwrap();

    let reader = ColocatedStorageReader::new(storage_reader, executor());

    assert_eq!(reader.latest_block_header().await.unwrap(), Some(header));
}

#[tokio::test]
async fn block_hash_returns_appended_block_hash() {
    let ((storage_reader, mut writer), _temp_dir) = get_test_storage();
    let header = BlockHeader::default();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &header)
        .unwrap()
        .commit()
        .unwrap();

    let reader = ColocatedStorageReader::new(storage_reader, executor());

    assert_eq!(reader.block_hash(BlockNumber(0)).await.unwrap(), header.block_hash);
}

#[tokio::test]
async fn block_hash_missing_block_is_block_not_found() {
    let ((storage_reader, _writer), _temp_dir) = get_test_storage();
    let reader = ColocatedStorageReader::new(storage_reader, executor());

    assert!(matches!(
        reader.block_hash(BlockNumber(7)).await,
        Err(FeederGatewayError::BlockNotFound)
    ));
}
