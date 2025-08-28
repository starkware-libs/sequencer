use apollo_class_manager_types::SharedClassManagerClient;
use apollo_network::network_manager::ClientResponsesManager;
use apollo_protobuf::sync::{DataOrFin, SignedBlockHeader};
use apollo_state_sync_metrics::metrics::{STATE_SYNC_HEADER_LATENCY_SEC, STATE_SYNC_HEADER_MARKER};
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use apollo_storage::{StorageError, StorageReader, StorageWriter};
use chrono::{TimeZone, Utc};
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::hash::StarkHash;
use tokio::time::{timeout, Duration};
use tracing::debug;

use super::block_data_stream_builder::{
    BadPeerError,
    BlockData,
    BlockDataStreamBuilder,
    BlockNumberLimit,
    ParseDataError,
};
use super::{P2pSyncClientError, ALLOWED_SIGNATURES_LENGTH};

impl BlockData for SignedBlockHeader {
    #[allow(clippy::as_conversions)] // FIXME: use int metrics so `as f64` may be removed.
    fn write_to_storage<'a>(
        self: Box<Self>,
        storage_writer: &'a mut StorageWriter,
        _class_manager_client: &'a mut SharedClassManagerClient,
    ) -> BoxFuture<'a, Result<(), P2pSyncClientError>> {
        async move {
            storage_writer
                .begin_rw_txn()?
                .append_header(
                    self.block_header.block_header_without_hash.block_number,
                    &self.block_header,
                )?
                .append_block_signature(
                    self.block_header.block_header_without_hash.block_number,
                    self
                    .signatures
                    // In the future we will support multiple signatures.
                    .first()
                    // The verification that the size of the vector is 1 is done in the data
                    // verification.
                    .expect("Vec::first should return a value on a vector of size 1"),
                )?
                .commit()?;
            STATE_SYNC_HEADER_MARKER.set_lossy(
                self.block_header.block_header_without_hash.block_number.unchecked_next().0,
            );
            // TODO(shahak): Fix code dup with central sync
            let time_delta = Utc::now()
                - Utc
                    .timestamp_opt(
                        self.block_header.block_header_without_hash.timestamp.0 as i64,
                        0,
                    )
                    .single()
                    .expect("block timestamp should be valid");
            let header_latency = time_delta.num_seconds();
            debug!("Header latency: {}.", header_latency);
            if header_latency >= 0 {
                STATE_SYNC_HEADER_LATENCY_SEC.set_lossy(header_latency);
            }
            Ok(())
        }
        .boxed()
    }
}

pub(crate) struct HeaderStreamBuilder;

impl BlockDataStreamBuilder<SignedBlockHeader> for HeaderStreamBuilder {
    type Output = SignedBlockHeader;

    const TYPE_DESCRIPTION: &'static str = "headers";
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::Unlimited;

    fn parse_data_for_block<'a>(
        signed_headers_response_manager: &'a mut ClientResponsesManager<
            DataOrFin<SignedBlockHeader>,
        >,
        block_number: BlockNumber,
        _storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, ParseDataError>> {
        async move {
            // TODO(noamsp): investigate and remove this timeout.
            let maybe_signed_header =
                timeout(Duration::from_secs(15), signed_headers_response_manager.next())
                    .await
                    .ok()
                    .flatten()
                    .ok_or(ParseDataError::BadPeer(BadPeerError::SessionEndedWithoutFin {
                        type_description: Self::TYPE_DESCRIPTION,
                    }))?;
            let Some(signed_block_header) = maybe_signed_header?.0 else {
                return Ok(None);
            };
            // TODO(shahak): Check that parent_hash is the same as the previous block's hash
            // and handle reverts.
            if block_number
                != signed_block_header.block_header.block_header_without_hash.block_number
            {
                return Err(ParseDataError::BadPeer(BadPeerError::HeadersUnordered {
                    expected_block_number: block_number,
                    actual_block_number: signed_block_header
                        .block_header
                        .block_header_without_hash
                        .block_number,
                }));
            }
            if signed_block_header.signatures.len() != ALLOWED_SIGNATURES_LENGTH {
                return Err(ParseDataError::BadPeer(BadPeerError::WrongSignaturesLength {
                    signatures: signed_block_header.signatures,
                }));
            }
            Ok(Some(signed_block_header))
        }
        .boxed()
    }

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError> {
        storage_reader.begin_ro_txn()?.get_header_marker()
    }

    // TODO(Eitan): Use real header once SyncBlock contains data required by full nodes
    fn convert_sync_block_to_block_data(
        block_number: BlockNumber,
        sync_block: SyncBlock,
    ) -> SignedBlockHeader {
        SignedBlockHeader {
            block_header: BlockHeader {
                block_hash: BlockHash(StarkHash::from(block_number.0)),
                block_header_without_hash: sync_block.block_header_without_hash,
                state_diff_length: Some(sync_block.state_diff.len()),
                n_transactions: sync_block.account_transaction_hashes.len()
                    + sync_block.l1_transaction_hashes.len(),
                ..Default::default()
            },
            signatures: vec![BlockSignature::default()],
        }
    }
}
