use chrono::{TimeZone, Utc};
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use metrics::gauge;
use papyrus_common::metrics as papyrus_metrics;
use papyrus_network::network_manager::ClientResponsesManager;
use papyrus_protobuf::sync::{DataOrFin, SignedBlockHeader};
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::hash::StarkHash;
use starknet_state_sync_types::state_sync_types::SyncBlock;
use tracing::debug;

use super::stream_builder::{
    BadPeerError,
    BlockData,
    BlockNumberLimit,
    DataStreamBuilder,
    ParseDataError,
};
use super::{P2pSyncClientError, ALLOWED_SIGNATURES_LENGTH, NETWORK_DATA_TIMEOUT};

impl BlockData for SignedBlockHeader {
    #[allow(clippy::as_conversions)] // FIXME: use int metrics so `as f64` may be removed.
    fn write_to_storage(
        self: Box<Self>,
        storage_writer: &mut StorageWriter,
    ) -> Result<(), StorageError> {
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
        gauge!(
            papyrus_metrics::PAPYRUS_HEADER_MARKER,
            self.block_header.block_header_without_hash.block_number.unchecked_next().0 as f64
        );
        // TODO(shahak): Fix code dup with central sync
        let time_delta = Utc::now()
            - Utc
                .timestamp_opt(self.block_header.block_header_without_hash.timestamp.0 as i64, 0)
                .single()
                .expect("block timestamp should be valid");
        let header_latency = time_delta.num_seconds();
        debug!("Header latency: {}.", header_latency);
        if header_latency >= 0 {
            gauge!(papyrus_metrics::PAPYRUS_HEADER_LATENCY_SEC, header_latency as f64);
        }
        Ok(())
    }
}

pub(crate) struct HeaderStreamBuilder;

impl DataStreamBuilder<SignedBlockHeader> for HeaderStreamBuilder {
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
            let maybe_signed_header =
                tokio::time::timeout(NETWORK_DATA_TIMEOUT, signed_headers_response_manager.next())
                    .await?
                    .ok_or(P2pSyncClientError::ReceiverChannelTerminated {
                        type_description: Self::TYPE_DESCRIPTION,
                    })?;
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
    ) -> Option<SignedBlockHeader> {
        Some(SignedBlockHeader {
            block_header: BlockHeader {
                block_hash: BlockHash(StarkHash::from(block_number.0)),
                block_header_without_hash: sync_block.block_header_without_hash,
                state_diff_length: Some(sync_block.state_diff.len()),
                n_transactions: sync_block.transaction_hashes.len(),
                ..Default::default()
            },
            signatures: vec![BlockSignature::default()],
        })
    }
}
