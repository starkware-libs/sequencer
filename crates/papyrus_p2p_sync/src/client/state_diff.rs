use std::collections::HashSet;

use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use metrics::gauge;
use papyrus_common::metrics as papyrus_metrics;
use papyrus_network::network_manager::ClientResponsesManager;
use papyrus_proc_macros::latency_histogram;
use papyrus_protobuf::sync::{DataOrFin, StateDiffChunk};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;
use starknet_state_sync_types::state_sync_types::SyncBlock;

use super::stream_builder::BadPeerError;
use crate::client::stream_builder::{
    BlockData,
    BlockNumberLimit,
    DataStreamBuilder,
    ParseDataError,
};
use crate::client::{P2pSyncClientError, NETWORK_DATA_TIMEOUT};

impl BlockData for (ThinStateDiff, BlockNumber) {
    #[latency_histogram("p2p_sync_state_diff_write_to_storage_latency_seconds", true)]
    #[allow(clippy::as_conversions)] // FIXME: use int metrics so `as f64` may be removed.
    fn write_to_storage(
        self: Box<Self>,
        storage_writer: &mut StorageWriter,
    ) -> Result<(), StorageError> {
        storage_writer.begin_rw_txn()?.append_state_diff(self.1, self.0)?.commit()?;
        gauge!(papyrus_metrics::PAPYRUS_STATE_MARKER, self.1.unchecked_next().0 as f64);
        Ok(())
    }
}

pub(crate) struct StateDiffStreamBuilder;

impl DataStreamBuilder<StateDiffChunk> for StateDiffStreamBuilder {
    type Output = (ThinStateDiff, BlockNumber);

    const TYPE_DESCRIPTION: &'static str = "state diffs";
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::HeaderMarker;

    #[latency_histogram("p2p_sync_state_diff_parse_data_for_block_latency_seconds", true)]
    fn parse_data_for_block<'a>(
        state_diff_chunks_response_manager: &'a mut ClientResponsesManager<
            DataOrFin<StateDiffChunk>,
        >,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, ParseDataError>> {
        async move {
            let mut result = ThinStateDiff::default();
            let mut prev_result_len = 0;
            let mut current_state_diff_len = 0;
            let target_state_diff_len = storage_reader
                .begin_ro_txn()?
                .get_block_header(block_number)?
                .expect("A header with number lower than the header marker is missing")
                .state_diff_length
                .ok_or(P2pSyncClientError::OldHeaderInStorage {
                    block_number,
                    missing_field: "state_diff_length",
                })?;

            while current_state_diff_len < target_state_diff_len {
                let maybe_state_diff_chunk = tokio::time::timeout(
                    NETWORK_DATA_TIMEOUT,
                    state_diff_chunks_response_manager.next(),
                )
                .await?
                .ok_or(P2pSyncClientError::ReceiverChannelTerminated {
                    type_description: Self::TYPE_DESCRIPTION,
                })?;
                let Some(state_diff_chunk) = maybe_state_diff_chunk?.0 else {
                    if current_state_diff_len == 0 {
                        return Ok(None);
                    } else {
                        return Err(ParseDataError::BadPeer(BadPeerError::WrongStateDiffLength {
                            expected_length: target_state_diff_len,
                            possible_lengths: vec![current_state_diff_len],
                        }));
                    }
                };
                prev_result_len = current_state_diff_len;
                if state_diff_chunk.is_empty() {
                    return Err(ParseDataError::BadPeer(BadPeerError::EmptyStateDiffPart));
                }
                // It's cheaper to calculate the length of `state_diff_part` than the length of
                // `result`.
                current_state_diff_len += state_diff_chunk.len();
                unite_state_diffs(&mut result, state_diff_chunk)?;
            }

            if current_state_diff_len != target_state_diff_len {
                return Err(ParseDataError::BadPeer(BadPeerError::WrongStateDiffLength {
                    expected_length: target_state_diff_len,
                    possible_lengths: vec![prev_result_len, current_state_diff_len],
                }));
            }

            validate_deprecated_declared_classes_non_conflicting(&result)?;
            Ok(Some((result, block_number)))
        }
        .boxed()
    }

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError> {
        storage_reader.begin_ro_txn()?.get_state_marker()
    }

    fn convert_sync_block_to_block_data(
        block_number: BlockNumber,
        sync_block: SyncBlock,
    ) -> Option<(ThinStateDiff, BlockNumber)> {
        Some((sync_block.state_diff, block_number))
    }
}

// For performance reasons, this function does not check if a deprecated class was declared twice.
// That check is done after we get the final state diff.
#[latency_histogram("p2p_sync_state_diff_unite_state_diffs_latency_seconds", true)]
fn unite_state_diffs(
    state_diff: &mut ThinStateDiff,
    state_diff_chunk: StateDiffChunk,
) -> Result<(), BadPeerError> {
    match state_diff_chunk {
        StateDiffChunk::ContractDiff(contract_diff) => {
            if let Some(class_hash) = contract_diff.class_hash {
                if state_diff
                    .deployed_contracts
                    .insert(contract_diff.contract_address, class_hash)
                    .is_some()
                {
                    return Err(BadPeerError::ConflictingStateDiffParts);
                }
            }
            if let Some(nonce) = contract_diff.nonce {
                if state_diff.nonces.insert(contract_diff.contract_address, nonce).is_some() {
                    return Err(BadPeerError::ConflictingStateDiffParts);
                }
            }
            if !contract_diff.storage_diffs.is_empty() {
                match state_diff.storage_diffs.get_mut(&contract_diff.contract_address) {
                    Some(storage_diffs) => {
                        for (k, v) in contract_diff.storage_diffs {
                            if storage_diffs.insert(k, v).is_some() {
                                return Err(BadPeerError::ConflictingStateDiffParts);
                            }
                        }
                    }
                    None => {
                        state_diff
                            .storage_diffs
                            .insert(contract_diff.contract_address, contract_diff.storage_diffs);
                    }
                }
            }
        }
        StateDiffChunk::DeclaredClass(declared_class) => {
            if state_diff
                .declared_classes
                .insert(declared_class.class_hash, declared_class.compiled_class_hash)
                .is_some()
            {
                return Err(BadPeerError::ConflictingStateDiffParts);
            }
        }
        StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class) => {
            state_diff.deprecated_declared_classes.push(deprecated_declared_class.class_hash);
        }
    }
    Ok(())
}

#[latency_histogram(
    "p2p_sync_state_diff_validate_deprecated_declared_classes_non_conflicting_latency_seconds",
    true
)]
fn validate_deprecated_declared_classes_non_conflicting(
    state_diff: &ThinStateDiff,
) -> Result<(), BadPeerError> {
    // TODO(shahak): Check if sorting is more efficient.
    if state_diff.deprecated_declared_classes.len()
        == state_diff.deprecated_declared_classes.iter().cloned().collect::<HashSet<_>>().len()
    {
        Ok(())
    } else {
        Err(BadPeerError::ConflictingStateDiffParts)
    }
}
