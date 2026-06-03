use std::sync::Arc;

use apollo_state_sync_types::communication::{MockStateSyncClient, StateSyncClientError};
use apollo_state_sync_types::errors::StateSyncError;
use starknet_api::block::{BlockHash, BlockNumber, BlockSignature};
use starknet_api::crypto::utils::Signature;
use starknet_api::hash::StarkHash;

use crate::errors::FeederGatewayError;
use crate::reader::remote::RemoteChainDataReader;
use crate::reader::ChainDataReader;

#[tokio::test]
async fn block_signature_returns_hash_and_signature_from_state_sync() {
    let expected_block_hash = BlockHash(StarkHash::from(0x1_u128));
    let expected_signature =
        BlockSignature(Signature { r: StarkHash::from(0x2_u128), s: StarkHash::from(0x3_u128) });

    let mut client = MockStateSyncClient::new();
    client.expect_get_block_hash().returning(move |_| Ok(expected_block_hash));
    client.expect_get_block_signature().returning(move |_| Ok(expected_signature));
    let reader = RemoteChainDataReader::new(Arc::new(client));

    let (block_hash, signature) = reader.block_signature(BlockNumber(7)).await.unwrap();

    assert_eq!(block_hash, expected_block_hash);
    assert_eq!(signature, expected_signature);
}

#[tokio::test]
async fn block_signature_of_unsynced_block_is_block_not_found() {
    let mut client = MockStateSyncClient::new();
    client.expect_get_block_hash().returning(|block_number| {
        Err(StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(block_number)))
    });
    let reader = RemoteChainDataReader::new(Arc::new(client));

    let result = reader.block_signature(BlockNumber(7)).await;

    assert!(matches!(result, Err(FeederGatewayError::BlockNotFound)));
}

#[tokio::test]
async fn block_number_by_hash_delegates_to_state_sync() {
    let mut client = MockStateSyncClient::new();
    client.expect_get_block_number_by_hash().returning(|_| Ok(Some(BlockNumber(7))));
    let reader = RemoteChainDataReader::new(Arc::new(client));

    let block_number =
        reader.block_number_by_hash(BlockHash(StarkHash::from(0x1_u128))).await.unwrap();

    assert_eq!(block_number, Some(BlockNumber(7)));
}

#[tokio::test]
async fn block_signature_missing_signature_is_block_not_found() {
    let mut client = MockStateSyncClient::new();
    client.expect_get_block_hash().returning(|_| Ok(BlockHash(StarkHash::from(0x1_u128))));
    client.expect_get_block_signature().returning(|block_number| {
        Err(StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(block_number)))
    });
    let reader = RemoteChainDataReader::new(Arc::new(client));

    let result = reader.block_signature(BlockNumber(7)).await;

    assert!(matches!(result, Err(FeederGatewayError::BlockNotFound)));
}
