// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod pending_sync;
pub mod sources;
#[cfg(test)]
mod sync_test;

use std::cmp::min;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use apollo_class_manager_types::{ClassManagerClientError, SharedClassManagerClient};
use apollo_config::converters::deserialize_seconds_to_duration;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_proc_macros::latency_histogram;
use apollo_starknet_client::reader::PendingData;
use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_BASE_LAYER_MARKER,
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    CENTRAL_SYNC_FORKS_FROM_FEEDER,
    STATE_SYNC_BODY_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
    STATE_SYNC_COMPILED_CLASS_MARKER,
    STATE_SYNC_HEADER_LATENCY_SEC,
    STATE_SYNC_HEADER_MARKER,
    STATE_SYNC_PROCESSED_TRANSACTIONS,
    STATE_SYNC_STATE_MARKER,
};
use apollo_storage::base_layer::{BaseLayerStorageReader, BaseLayerStorageWriter};
use apollo_storage::body::BodyStorageWriter;
use apollo_storage::class::{ClassStorageReader, ClassStorageWriter};
use apollo_storage::class_manager::{ClassManagerStorageReader, ClassManagerStorageWriter};
use apollo_storage::compiled_class::{CasmStorageReader, CasmStorageWriter};
use apollo_storage::db::DbError;
use apollo_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use apollo_storage::state::{StateStorageReader, StateStorageWriter};
use apollo_storage::{StorageError, StorageReader, StorageWriter};
use async_stream::try_stream;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use chrono::{TimeZone, Utc};
use futures::future::pending;
use futures::stream;
use futures_util::{pin_mut, select, Stream, StreamExt};
use indexmap::IndexMap;
use papyrus_common::pending_classes::PendingClasses;
use serde::{Deserialize, Serialize};
use sources::base_layer::BaseLayerSourceError;
use starknet_api::block::{
    Block,
    BlockHash,
    BlockHashAndNumber,
    BlockNumber,
    BlockSignature,
    StarknetVersion,
};
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, SequencerPublicKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{StateDiff, ThinStateDiff};
use tokio::sync::{Mutex, RwLock};
use tokio::task::{spawn_blocking, JoinError};
use tracing::{debug, error, info, instrument, trace, warn};

use crate::pending_sync::sync_pending_data;
use crate::sources::base_layer::{BaseLayerSourceTrait, EthereumBaseLayerSource};
use crate::sources::central::{CentralError, CentralSource, CentralSourceTrait};
use crate::sources::pending::{PendingError, PendingSource, PendingSourceTrait};

// TODO(shahak): Consider adding genesis hash to the config to support chains that have
// different genesis hash.
// TODO(Shahak): Consider moving to a more general place.
pub const GENESIS_HASH: &str = "0x0";

// TODO(dvir): add to config.
// Sleep duration between polling for pending data.
const PENDING_SLEEP_DURATION: Duration = Duration::from_millis(500);

// Sleep duration, in seconds, between sync progress checks.
const SLEEP_TIME_SYNC_PROGRESS: Duration = Duration::from_secs(300);

// The first starknet version where we can send sierras to the class manager without casms and it
// will compile them, in a backward-compatible manner.
const STARKNET_VERSION_TO_COMPILE_FROM: StarknetVersion = StarknetVersion::V0_12_0;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SyncConfig {
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub block_propagation_sleep_duration: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub base_layer_propagation_sleep_duration: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub recoverable_error_sleep_duration: Duration,
    pub blocks_max_stream_size: u32,
    pub state_updates_max_stream_size: u32,
    pub verify_blocks: bool,
    pub collect_pending_data: bool,
    pub store_sierras_and_casms: bool,
}

impl SerializeConfig for SyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "block_propagation_sleep_duration",
                &self.block_propagation_sleep_duration.as_secs(),
                "Time in seconds before checking for a new block after the node is synchronized.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "base_layer_propagation_sleep_duration",
                &self.base_layer_propagation_sleep_duration.as_secs(),
                "Time in seconds to poll the base layer to get the latest proved block.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "recoverable_error_sleep_duration",
                &self.recoverable_error_sleep_duration.as_secs(),
                "Waiting time in seconds before restarting synchronization after a recoverable \
                 error.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "blocks_max_stream_size",
                &self.blocks_max_stream_size,
                "Max amount of blocks to download in a stream.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "state_updates_max_stream_size",
                &self.state_updates_max_stream_size,
                "Max amount of state updates to download in a stream.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "verify_blocks",
                &self.verify_blocks,
                "Whether to verify incoming blocks.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "collect_pending_data",
                &self.collect_pending_data,
                "Whether to collect data on pending blocks.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "store_sierras_and_casms",
                &self.store_sierras_and_casms,
                "Whether to store sierras and casms to the storage. This allows maintaining \
                 backward-compatibility with native-blockifier",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            block_propagation_sleep_duration: Duration::from_secs(2),
            base_layer_propagation_sleep_duration: Duration::from_secs(10),
            recoverable_error_sleep_duration: Duration::from_secs(3),
            blocks_max_stream_size: 1000,
            state_updates_max_stream_size: 1000,
            verify_blocks: true,
            collect_pending_data: false,
            store_sierras_and_casms: false,
        }
    }
}

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and writes to Storage and shared
// memory.
pub struct GenericStateSync<
    TCentralSource: CentralSourceTrait + Sync + Send,
    TPendingSource: PendingSourceTrait + Sync + Send,
    TBaseLayerSource: BaseLayerSourceTrait + Sync + Send,
> {
    config: SyncConfig,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pending_data: Arc<RwLock<PendingData>>,
    central_source: Arc<TCentralSource>,
    pending_source: Arc<TPendingSource>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    base_layer_source: Option<Arc<TBaseLayerSource>>,
    reader: StorageReader,
    writer: Arc<Mutex<StorageWriter>>,
    sequencer_pub_key: Option<SequencerPublicKey>,
    class_manager_client: Option<SharedClassManagerClient>,
}

pub type StateSyncResult = Result<(), StateSyncError>;

// TODO(DanB): Sort alphabetically.
// TODO(DanB): Change this to CentralStateSyncError
#[derive(thiserror::Error, Debug)]
pub enum StateSyncError {
    #[error("Sync stopped progress.")]
    NoProgress,
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    CentralSourceError(#[from] CentralError),
    #[error(transparent)]
    PendingSourceError(#[from] PendingError),
    #[error(
        "Parent block hash of block {block_number} is not consistent with the stored block. \
         Expected {expected_parent_block_hash}, found {stored_parent_block_hash}."
    )]
    ParentBlockHashMismatch {
        block_number: BlockNumber,
        expected_parent_block_hash: BlockHash,
        stored_parent_block_hash: BlockHash,
    },
    #[error("Header for block {block_number} wasn't found when trying to store base layer block.")]
    BaseLayerBlockWithoutMatchingHeader { block_number: BlockNumber },
    #[error(transparent)]
    BaseLayerSourceError(#[from] BaseLayerSourceError),
    #[error(
        "For {block_number} base layer and l2 doesn't match. Base layer hash: {base_layer_hash}, \
         L2 hash: {l2_hash}."
    )]
    BaseLayerHashMismatch {
        block_number: BlockNumber,
        base_layer_hash: BlockHash,
        l2_hash: BlockHash,
    },
    #[error("Sequencer public key changed from {old:?} to {new:?}.")]
    SequencerPubKeyChanged { old: SequencerPublicKey, new: SequencerPublicKey },
    #[error(transparent)]
    ClassManagerClientError(#[from] ClassManagerClientError),
    #[error(transparent)]
    JoinError(#[from] JoinError),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SyncEvent {
    NoProgress,
    BlockAvailable {
        block_number: BlockNumber,
        block: Block,
        signature: BlockSignature,
    },
    StateDiffAvailable {
        block_number: BlockNumber,
        block_hash: BlockHash,
        state_diff: StateDiff,
        // TODO(anatg): Remove once there are no more deployed contracts with undeclared classes.
        // Class definitions of deployed contracts with classes that were not declared in this
        // state diff.
        // Note: Since 0.11 new classes can not be implicitly declared.
        deployed_contract_class_definitions: IndexMap<ClassHash, DeprecatedContractClass>,
    },
    CompiledClassAvailable {
        class_hash: ClassHash,
        compiled_class_hash: CompiledClassHash,
        compiled_class: CasmContractClass,
        is_compiler_backward_compatible: bool,
    },
    NewBaseLayerBlock {
        block_number: BlockNumber,
        block_hash: BlockHash,
    },
}

impl<
    TCentralSource: CentralSourceTrait + Sync + Send + 'static,
    TPendingSource: PendingSourceTrait + Sync + Send + 'static,
    TBaseLayerSource: BaseLayerSourceTrait + Sync + Send,
> GenericStateSync<TCentralSource, TPendingSource, TBaseLayerSource>
{
    pub async fn run(mut self) -> StateSyncResult {
        info!("State sync started.");
        loop {
            match self.sync_while_ok().await {
                // A recoverable error occurred. Sleep and try syncing again.
                Err(err) if is_recoverable(&err) => {
                    warn!("Recoverable error encountered while syncing, error: {}", err);
                    tokio::time::sleep(self.config.recoverable_error_sleep_duration).await;
                    continue;
                }
                // Unrecoverable errors.
                Err(err) => {
                    error!("Fatal error while syncing: {}", err);
                    return Err(err);
                }
                Ok(_) => {
                    unreachable!("Sync should either return with an error or continue forever.")
                }
            }
        }

        // Whitelisting of errors from which we might be able to recover.
        fn is_recoverable(err: &StateSyncError) -> bool {
            // We don't use here catch-all pattern to enforce conscious decision for each error
            // kind.
            match err {
                StateSyncError::StorageError(error) => {
                    matches!(error, StorageError::InnerError(_))
                }
                StateSyncError::NoProgress
                | StateSyncError::CentralSourceError(_)
                | StateSyncError::PendingSourceError(_)
                | StateSyncError::BaseLayerSourceError(_)
                | StateSyncError::ParentBlockHashMismatch { .. }
                | StateSyncError::BaseLayerHashMismatch { .. }
                | StateSyncError::ClassManagerClientError(_)
                | StateSyncError::BaseLayerBlockWithoutMatchingHeader { .. }
                | StateSyncError::JoinError(_) => true,
                StateSyncError::SequencerPubKeyChanged { .. } => false,
            }
        }
    }

    async fn track_sequencer_public_key_changes(&mut self) -> StateSyncResult {
        let sequencer_pub_key = self.central_source.get_sequencer_pub_key().await?;
        match self.sequencer_pub_key {
            // First time setting the sequencer public key.
            None => {
                info!("Sequencer public key set to {sequencer_pub_key:?}.");
                self.sequencer_pub_key = Some(sequencer_pub_key);
            }
            Some(cur_key) => {
                if cur_key != sequencer_pub_key {
                    warn!(
                        "Sequencer public key changed from {cur_key:?} to {sequencer_pub_key:?}."
                    );
                    // TODO(Yair): Add alert.
                    self.sequencer_pub_key = Some(sequencer_pub_key);
                    return Err(StateSyncError::SequencerPubKeyChanged {
                        old: cur_key,
                        new: sequencer_pub_key,
                    });
                }
            }
        };
        Ok(())
    }

    // Sync until encountering an error:
    //  1. If needed, revert blocks from the end of the chain.
    //  2. Create infinite block and state diff streams to fetch data from the central source.
    //  3. Fetch data from the streams with unblocking wait while there is no new data.
    async fn sync_while_ok(&mut self) -> StateSyncResult {
        if self.config.verify_blocks {
            self.track_sequencer_public_key_changes().await?;
        }
        self.handle_block_reverts().await?;
        let block_stream = stream_new_blocks(
            self.reader.clone(),
            self.central_source.clone(),
            self.pending_source.clone(),
            self.shared_highest_block.clone(),
            self.pending_data.clone(),
            self.pending_classes.clone(),
            self.config.block_propagation_sleep_duration,
            self.config.collect_pending_data,
            PENDING_SLEEP_DURATION,
            self.config.blocks_max_stream_size,
        )
        .fuse();
        let state_diff_stream = stream_new_state_diffs(
            self.reader.clone(),
            self.central_source.clone(),
            self.config.block_propagation_sleep_duration,
            self.config.state_updates_max_stream_size,
        )
        .fuse();
        let compiled_class_stream = stream_new_compiled_classes(
            self.reader.clone(),
            self.central_source.clone(),
            self.config.block_propagation_sleep_duration,
            // TODO(yair): separate config param.
            self.config.state_updates_max_stream_size,
            self.config.store_sierras_and_casms,
        )
        .fuse();
        let base_layer_block_stream = match &self.base_layer_source {
            Some(base_layer_source) => stream_new_base_layer_block(
                self.reader.clone(),
                base_layer_source.clone(),
                self.config.base_layer_propagation_sleep_duration,
            )
            .boxed()
            .fuse(),
            None => stream::pending().boxed().fuse(),
        };
        // TODO(dvir): try use interval instead of stream.
        // TODO(DvirYo): fix the bug and remove this check.
        let check_sync_progress =
            check_sync_progress(self.reader.clone(), self.config.store_sierras_and_casms).fuse();
        pin_mut!(
            block_stream,
            state_diff_stream,
            compiled_class_stream,
            base_layer_block_stream,
            check_sync_progress
        );

        loop {
            debug!("Selecting between block sync and state diff sync.");
            let sync_event = select! {
              res = block_stream.next() => res,
              res = state_diff_stream.next() => res,
              res = compiled_class_stream.next() => res,
              res = base_layer_block_stream.next() => res,
              res = check_sync_progress.next() => res,
              complete => break,
            }
            .expect("Received None as a sync event.")?;
            self.process_sync_event(sync_event).await?;
            debug!("Finished processing sync event.");
        }
        unreachable!("Fetching data loop should never return.");
    }

    // Tries to store the incoming data.
    async fn process_sync_event(&mut self, sync_event: SyncEvent) -> StateSyncResult {
        match sync_event {
            SyncEvent::BlockAvailable { block_number, block, signature } => {
                self.store_block(block_number, block, signature).await
            }
            SyncEvent::StateDiffAvailable {
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            } => {
                self.store_state_diff(
                    block_number,
                    block_hash,
                    state_diff,
                    deployed_contract_class_definitions,
                )
                .await
            }
            SyncEvent::CompiledClassAvailable {
                class_hash,
                compiled_class_hash,
                compiled_class,
                is_compiler_backward_compatible,
            } => {
                self.store_compiled_class(
                    class_hash,
                    compiled_class_hash,
                    compiled_class,
                    is_compiler_backward_compatible,
                )
                .await
            }
            SyncEvent::NewBaseLayerBlock { block_number, block_hash } => {
                self.store_base_layer_block(block_number, block_hash).await
            }
            SyncEvent::NoProgress => Err(StateSyncError::NoProgress),
        }
    }

    #[latency_histogram("sync_store_block_latency_seconds", false)]
    #[instrument(
        skip(self, block),
        level = "debug",
        fields(block_hash = format_args!("{:#064x}", block.header.block_hash.0)),
        err
    )]
    #[allow(clippy::as_conversions)] // FIXME: use int metrics so `as f64` may be removed.
    async fn store_block(
        &mut self,
        block_number: BlockNumber,
        block: Block,
        signature: BlockSignature,
    ) -> StateSyncResult {
        // Assuming the central source is trusted, detect reverts by comparing the incoming block's
        // parent hash to the current hash.
        self.verify_parent_block_hash(block_number, &block)?;

        debug!("Storing block number: {block_number}, block header: {:?}", block.header);
        trace!("Block data: {block:#?}, signature: {signature:?}");
        let num_txs =
            block.body.transactions.len().try_into().expect("Failed to convert usize to u64");
        let timestamp = block.header.block_header_without_hash.timestamp;
        self.perform_storage_writes(move |writer| {
            let mut txn = writer
                .begin_rw_txn()?
                .append_header(block_number, &block.header)?
                .append_block_signature(block_number, &signature)?
                .append_body(block_number, block.body)?;
            if block.header.block_header_without_hash.starknet_version
                < STARKNET_VERSION_TO_COMPILE_FROM
            {
                txn = txn.update_compiler_backward_compatibility_marker(
                    &block_number.unchecked_next(),
                )?;
            }
            txn.commit()?;
            Ok(())
        })
        .await?;
        STATE_SYNC_HEADER_MARKER.set_lossy(block_number.unchecked_next().0);
        STATE_SYNC_BODY_MARKER.set_lossy(block_number.unchecked_next().0);
        STATE_SYNC_PROCESSED_TRANSACTIONS.increment(num_txs);
        let time_delta = Utc::now()
            - Utc
                .timestamp_opt(timestamp.0 as i64, 0)
                .single()
                .expect("block timestamp should be valid");
        let header_latency = time_delta.num_seconds();
        debug!("Header latency: {}.", header_latency);
        if header_latency >= 0 {
            STATE_SYNC_HEADER_LATENCY_SEC.set_lossy(header_latency);
        }

        Ok(())
    }

    #[latency_histogram("sync_store_state_diff_latency_seconds", false)]
    #[instrument(skip(self, state_diff, deployed_contract_class_definitions), level = "debug", err)]
    async fn store_state_diff(
        &mut self,
        block_number: BlockNumber,
        block_hash: BlockHash,
        state_diff: StateDiff,
        deployed_contract_class_definitions: IndexMap<ClassHash, DeprecatedContractClass>,
    ) -> StateSyncResult {
        // TODO(dan): verifications - verify state diff against stored header.
        debug!("Storing state diff.");
        trace!("StateDiff data: {state_diff:#?}");

        // TODO(shahak): split the state diff stream to 2 separate streams for blocks and for
        // classes.
        let (thin_state_diff, classes, deprecated_classes) =
            ThinStateDiff::from_state_diff(state_diff);

        // Sending to class manager before updating the storage so that if the class manager send
        // fails we retry the same block.
        if let Some(class_manager_client) = &self.class_manager_client {
            // Blocks smaller than compiler_backward_compatibility marker are added to class
            // manager via the compiled classes stream.
            // We're sure that if the current block is above the compiler_backward_compatibility
            // marker then the compiler_backward_compatibility will not advance anymore, because
            // the compiler_backward_compatibility marker advances in the header stream and this
            // stream is behind the header stream
            // The compiled classes stream is always behind the compiler_backward_compatibility
            // marker
            // TODO(shahak): Consider storing a boolean and updating it to true once
            // compiler_backward_compatibility_marker <= block_number and avoiding the check if the
            // boolean is true.
            let compiler_backward_compatibility_marker =
                self.reader.begin_ro_txn()?.get_compiler_backward_compatibility_marker()?;

            if compiler_backward_compatibility_marker <= block_number {
                for (expected_class_hash, class) in &classes {
                    let class_hash =
                        class_manager_client.add_class(class.clone()).await?.class_hash;
                    if class_hash != *expected_class_hash {
                        panic!(
                            "Class hash mismatch. Expected: {expected_class_hash}, got: \
                             {class_hash}."
                        );
                    }
                }
            }

            for (class_hash, deprecated_class) in &deprecated_classes {
                class_manager_client
                    .add_deprecated_class(*class_hash, deprecated_class.clone())
                    .await?;
            }
        }
        let has_class_manager = self.class_manager_client.is_some();
        let store_sierras_and_casms = self.config.store_sierras_and_casms;
        self.perform_storage_writes(move |writer| {
            if has_class_manager {
                writer
                    .begin_rw_txn()?
                    .update_class_manager_block_marker(&block_number.unchecked_next())?
                    .commit()?;
                STATE_SYNC_CLASS_MANAGER_MARKER.set_lossy(block_number.unchecked_next().0);
            }
            let mut txn = writer.begin_rw_txn()?;
            txn = txn.append_state_diff(block_number, thin_state_diff)?;
            if store_sierras_and_casms {
                txn = txn.append_classes(
                    block_number,
                    &classes
                        .iter()
                        .map(|(class_hash, class)| (*class_hash, class))
                        .collect::<Vec<_>>(),
                    &deprecated_classes
                        .iter()
                        .chain(deployed_contract_class_definitions.iter())
                        .map(|(class_hash, deprecated_class)| (*class_hash, deprecated_class))
                        .collect::<Vec<_>>(),
                )?;
            }
            txn.commit()?;
            Ok(())
        })
        .await?;

        let compiled_class_marker = self.reader.begin_ro_txn()?.get_compiled_class_marker()?;
        STATE_SYNC_STATE_MARKER.set_lossy(block_number.unchecked_next().0);
        STATE_SYNC_COMPILED_CLASS_MARKER.set_lossy(compiled_class_marker.0);

        // Info the user on syncing the block once all the data is stored.
        info!("SYNC_NEW_BLOCK: Added block {} with hash {:#064x}.", block_number, block_hash.0);

        Ok(())
    }

    #[latency_histogram("sync_store_compiled_class_latency_seconds", false)]
    #[instrument(skip(self, compiled_class), level = "debug", err)]
    async fn store_compiled_class(
        &mut self,
        class_hash: ClassHash,
        _compiled_class_hash_v1: CompiledClassHash,
        compiled_class: CasmContractClass,
        is_compiler_backward_compatible: bool,
    ) -> StateSyncResult {
        let compiled_class_hash_v2 = compiled_class.hash(&HashVersion::V2);

        if !is_compiler_backward_compatible {
            if let Some(class_manager_client) = &self.class_manager_client {
                let class = self.reader.begin_ro_txn()?.get_class(&class_hash)?.expect(
                    "Compiled classes stream gave class hash that doesn't appear in storage.",
                );
                let sierra_version = SierraVersion::extract_from_program(&class.sierra_program)
                    .expect("Failed reading sierra version from program.");
                let contract_class = ContractClass::V1((compiled_class.clone(), sierra_version));
                class_manager_client
                    .add_class_and_executable_unsafe(
                        class_hash,
                        class,
                        compiled_class_hash_v2,
                        contract_class,
                    )
                    .await
                    .expect("Failed adding class and compiled class to class manager.");
            }
        }
        if !self.config.store_sierras_and_casms {
            return Ok(());
        }
        let result = self
            .perform_storage_writes(move |writer| {
                writer.begin_rw_txn()?.append_casm(&class_hash, &compiled_class)?.commit()?;
                Ok(())
            })
            .await;
        // TODO(Yair): verifications - verify casm corresponds to a class on storage.
        match result {
            Ok(()) => {
                let compiled_class_marker =
                    self.reader.begin_ro_txn()?.get_compiled_class_marker()?;
                // Write class and casm to class manager.
                STATE_SYNC_COMPILED_CLASS_MARKER.set_lossy(compiled_class_marker.0);
                debug!("Added compiled class.");
                Ok(())
            }
            // TODO(yair): Modify the stream so it skips already stored classes.
            // Compiled classes rewrite is valid because the stream downloads from the beginning
            // of the block instead of the last downloaded class.
            Err(StateSyncError::StorageError(StorageError::InnerError(
                DbError::KeyAlreadyExists(..),
            ))) => {
                debug!("Compiled class of {class_hash} already stored.");
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    #[instrument(skip(self), level = "debug", err)]
    // In case of a mismatch between the base layer and l2, an error will be returned, then the
    // sync will revert blocks if needed based on the l2 central source. This approach works as long
    // as l2 is trusted so all the reverts can be detect by using it.
    async fn store_base_layer_block(
        &mut self,
        block_number: BlockNumber,
        block_hash: BlockHash,
    ) -> StateSyncResult {
        self.perform_storage_writes(move |writer| {
            let txn = writer.begin_rw_txn()?;
            // Missing header can be because of a base layer reorg, the matching header may be
            // reverted.
            let expected_hash = txn
                .get_block_header(block_number)?
                .ok_or(StateSyncError::BaseLayerBlockWithoutMatchingHeader { block_number })?
                .block_hash;
            // Can be caused because base layer reorg or l2 reverts.
            if expected_hash != block_hash {
                return Err(StateSyncError::BaseLayerHashMismatch {
                    block_number,
                    base_layer_hash: block_hash,
                    l2_hash: expected_hash,
                });
            }
            if txn.get_base_layer_block_marker()? != block_number.unchecked_next() {
                info!("Verified block {block_number} hash against base layer.");
                txn.update_base_layer_block_marker(&block_number.unchecked_next())?.commit()?;
                CENTRAL_SYNC_BASE_LAYER_MARKER.set_lossy(block_number.unchecked_next().0);
            }
            Ok(())
        })
        .await
    }

    // Compares the block's parent hash to the stored block.
    fn verify_parent_block_hash(
        &self,
        block_number: BlockNumber,
        block: &Block,
    ) -> StateSyncResult {
        let prev_block_number = match block_number.prev() {
            None => return Ok(()),
            Some(bn) => bn,
        };
        let prev_hash = self
            .reader
            .begin_ro_txn()?
            .get_block_header(prev_block_number)?
            .ok_or(StorageError::DBInconsistency {
                msg: format!(
                    "Missing block {prev_block_number} in the storage (for verifying block \
                     {block_number}).",
                ),
            })?
            .block_hash;

        if prev_hash != block.header.block_header_without_hash.parent_hash {
            // A revert detected, log and restart sync loop.
            warn!(
                "Detected revert while processing block {}. Parent hash of the incoming block is \
                 {}, current block hash is {}.",
                block_number, block.header.block_header_without_hash.parent_hash, prev_hash
            );
            CENTRAL_SYNC_FORKS_FROM_FEEDER.increment(1);
            return Err(StateSyncError::ParentBlockHashMismatch {
                block_number,
                expected_parent_block_hash: block.header.block_header_without_hash.parent_hash,
                stored_parent_block_hash: prev_hash,
            });
        }

        Ok(())
    }

    // Reverts data if needed.
    async fn handle_block_reverts(&mut self) -> Result<(), StateSyncError> {
        debug!("Handling block reverts.");
        let header_marker = self.reader.begin_ro_txn()?.get_header_marker()?;

        // Revert last blocks if needed.
        let mut last_block_in_storage = header_marker.prev();
        while let Some(block_number) = last_block_in_storage {
            if self.should_revert_block(block_number).await? {
                self.revert_block(block_number).await?;
                last_block_in_storage = block_number.prev();
            } else {
                break;
            }
        }
        Ok(())
    }

    // TODO(dan): update necessary metrics.
    // Deletes the block data from the storage.
    #[allow(clippy::expect_fun_call)]
    #[instrument(skip(self), level = "debug", err)]
    async fn revert_block(&mut self, block_number: BlockNumber) -> StateSyncResult {
        debug!("Reverting block.");

        self.perform_storage_writes(move |writer| {
            let mut txn = writer.begin_rw_txn()?;
            txn = txn.try_revert_base_layer_marker(block_number)?;
            let res = txn.revert_header(block_number)?;
            txn = res.0;
            let mut reverted_block_hash: Option<BlockHash> = None;
            if let Some(header) = res.1 {
                reverted_block_hash = Some(header.block_hash);

                let res = txn.revert_body(block_number)?;
                txn = res.0;

                let res = txn.revert_state_diff(block_number)?;
                txn = res.0;
            }

            txn.commit()?;
            if let Some(hash) = reverted_block_hash {
                info!(%hash, %block_number, "Reverted block.");
            }
            Ok(())
        })
        .await
    }

    /// Checks if centrals block hash at the block number is different from ours (or doesn't exist).
    /// If so, a revert is required.
    async fn should_revert_block(&self, block_number: BlockNumber) -> Result<bool, StateSyncError> {
        if let Some(central_block_hash) = self.central_source.get_block_hash(block_number).await? {
            let storage_block_header =
                self.reader.begin_ro_txn()?.get_block_header(block_number)?;

            match storage_block_header {
                Some(block_header) => Ok(block_header.block_hash != central_block_hash),
                None => Ok(false),
            }
        } else {
            // Block number doesn't exist in central, revert.
            Ok(true)
        }
    }

    async fn perform_storage_writes<
        F: FnOnce(&mut StorageWriter) -> Result<(), StateSyncError> + Send + 'static,
    >(
        &mut self,
        f: F,
    ) -> Result<(), StateSyncError> {
        let writer = self.writer.clone();
        spawn_blocking(move || f(&mut (writer.blocking_lock()))).await?
    }
}
// TODO(dvir): consider gathering in a single pending argument instead.
#[allow(clippy::too_many_arguments)]
fn stream_new_blocks<
    TCentralSource: CentralSourceTrait + Sync + Send + 'static,
    TPendingSource: PendingSourceTrait + Sync + Send + 'static,
>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    pending_source: Arc<TPendingSource>,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    block_propagation_sleep_duration: Duration,
    collect_pending_data: bool,
    pending_sleep_duration: Duration,
    max_stream_size: u32,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
            loop {
            let header_marker = reader.begin_ro_txn()?.get_header_marker()?;
            let latest_central_block = central_source.get_latest_block().await?;
            *shared_highest_block.write().await = latest_central_block;
            let central_block_marker = latest_central_block.map_or(
                BlockNumber::default(), |block_hash_and_number| block_hash_and_number.number.unchecked_next()
            );
            CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.set_lossy(central_block_marker.0);
            if header_marker == central_block_marker {
                // Only if the node have the last block and state (without casms), sync pending data.
                if collect_pending_data && reader.begin_ro_txn()?.get_state_marker()? == header_marker{
                    // Here is the only place we update the pending data.
                    debug!("Start polling for pending data of block {:?}.", header_marker);
                    sync_pending_data(
                        reader.clone(),
                        central_source.clone(),
                        pending_source.clone(),
                        pending_data.clone(),
                        pending_classes.clone(),
                        pending_sleep_duration,
                    ).await?;
                }
                else{
                    trace!("Blocks syncing reached the last known block {:?}, waiting for blockchain to advance.", header_marker.prev());
                    tokio::time::sleep(block_propagation_sleep_duration).await;
                };
                continue;
            }
            let up_to = min(central_block_marker, BlockNumber(header_marker.0 + u64::from(max_stream_size)));
            debug!("Downloading blocks [{} - {}).", header_marker, up_to);
            let block_stream =
                central_source.stream_new_blocks(header_marker, up_to).fuse();
            pin_mut!(block_stream);
            while let Some(maybe_block) = block_stream.next().await {
                let (block_number, block, signature) = maybe_block?;
                yield SyncEvent::BlockAvailable { block_number, block , signature };
            }
        }
    }
}

fn stream_new_state_diffs<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propagation_sleep_duration: Duration,
    max_stream_size: u32,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        loop {
            let txn = reader.begin_ro_txn()?;
            let state_marker = txn.get_state_marker()?;
            let last_block_number = txn.get_header_marker()?;
            drop(txn);
            if state_marker == last_block_number {
                trace!("State updates syncing reached the last downloaded block {:?}, waiting for more blocks.", state_marker.prev());
                tokio::time::sleep(block_propagation_sleep_duration).await;
                continue;
            }
            let up_to = min(last_block_number, BlockNumber(state_marker.0 + u64::from(max_stream_size)));
            debug!("Downloading state diffs [{} - {}).", state_marker, up_to);
            let state_diff_stream =
                central_source.stream_state_updates(state_marker, up_to).fuse();
            pin_mut!(state_diff_stream);

            while let Some(maybe_state_diff) = state_diff_stream.next().await {
                let (
                    block_number,
                    block_hash,
                    mut state_diff,
                    deployed_contract_class_definitions,
                ) = maybe_state_diff?;
                sort_state_diff(&mut state_diff);
                yield SyncEvent::StateDiffAvailable {
                    block_number,
                    block_hash,
                    state_diff,
                    deployed_contract_class_definitions,
                };
            }
        }
    }
}

pub fn sort_state_diff(diff: &mut StateDiff) {
    diff.declared_classes.sort_unstable_keys();
    diff.deprecated_declared_classes.sort_unstable_keys();
    diff.deployed_contracts.sort_unstable_keys();
    diff.nonces.sort_unstable_keys();
    diff.storage_diffs.sort_unstable_keys();
    for storage_entries in diff.storage_diffs.values_mut() {
        storage_entries.sort_unstable_keys();
    }
}

pub type StateSync = GenericStateSync<CentralSource, PendingSource, EthereumBaseLayerSource>;

impl StateSync {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: SyncConfig,
        shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
        pending_data: Arc<RwLock<PendingData>>,
        pending_classes: Arc<RwLock<PendingClasses>>,
        central_source: CentralSource,
        pending_source: PendingSource,
        base_layer_source: Option<EthereumBaseLayerSource>,
        reader: StorageReader,
        writer: StorageWriter,
        class_manager_client: Option<SharedClassManagerClient>,
    ) -> Self {
        let base_layer_source = base_layer_source.map(Arc::new);
        Self {
            config,
            shared_highest_block,
            pending_data,
            pending_classes,
            central_source: Arc::new(central_source),
            pending_source: Arc::new(pending_source),
            base_layer_source,
            reader,
            writer: Arc::new(Mutex::new(writer)),
            sequencer_pub_key: None,
            class_manager_client,
        }
    }
}

fn stream_new_compiled_classes<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propagation_sleep_duration: Duration,
    max_stream_size: u32,
    store_sierras_and_casms: bool,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        loop {
            let txn = reader.begin_ro_txn()?;
            let mut from = txn.get_compiled_class_marker()?;
            let state_marker = txn.get_state_marker()?;
            let compiler_backward_compatibility_marker = txn.get_compiler_backward_compatibility_marker()?;
            // Avoid starting streams from blocks without declared classes.
            while from < state_marker {
                let state_diff = txn.get_state_diff(from)?.expect("Expecting to have state diff up to the marker.");
                if state_diff.declared_classes.is_empty() {
                    from = from.unchecked_next();
                }
                else {
                    break;
                }
            }

            if from == state_marker {
                debug!(
                    "Compiled classes syncing reached the last downloaded state update{:?}, waiting \
                     for more state updates.", state_marker.prev()
                );
                tokio::time::sleep(block_propagation_sleep_duration).await;
                continue;
            }
            let mut up_to = min(state_marker, BlockNumber(from.0 + u64::from(max_stream_size)));
            let are_casms_backward_compatible = from >= compiler_backward_compatibility_marker;
            // We want that the stream will either have all compiled classes as backward compatible
            // or all as not backward compatible. If needed we'll decrease up_to
            if from < compiler_backward_compatibility_marker && up_to > compiler_backward_compatibility_marker {
                up_to = compiler_backward_compatibility_marker;
            }

            // No point in downloading casms if we don't store them and don't send them to the
            // class manager
            if are_casms_backward_compatible && !store_sierras_and_casms {
                info!("Compiled classes stream reached a block that has backward compatibility for \
                      the compiler, and store_sierras_and_casms is set to false. \
                      Finishing the compiled class stream");
                pending::<()>().await;
                continue;
            }

            debug!("Downloading compiled classes of blocks [{} - {}).", from, up_to);
            let compiled_classes_stream =
                central_source.stream_compiled_classes(from, up_to).fuse();
            pin_mut!(compiled_classes_stream);

            while let Some(maybe_compiled_class) = compiled_classes_stream.next().await {
                let (class_hash, compiled_class_hash, compiled_class) = maybe_compiled_class?;
                yield SyncEvent::CompiledClassAvailable {
                    class_hash,
                    compiled_class_hash,
                    compiled_class,
                    is_compiler_backward_compatible: are_casms_backward_compatible,
                };
            }
        }
    }
}

// TODO(dvir): consider combine this function and store_base_layer_block.
fn stream_new_base_layer_block<TBaseLayerSource: BaseLayerSourceTrait + Sync>(
    reader: StorageReader,
    base_layer_source: Arc<TBaseLayerSource>,
    base_layer_propagation_sleep_duration: Duration,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        loop {
            tokio::time::sleep(base_layer_propagation_sleep_duration).await;
            let txn = reader.begin_ro_txn()?;
            let header_marker = txn.get_header_marker()?;
            match base_layer_source.latest_proved_block().await? {
                Some((block_number, _block_hash)) if header_marker <= block_number => {
                    debug!(
                        "Sync headers ({header_marker}) is behind the base layer tip \
                         ({block_number}), waiting for sync to advance."
                    );
                }
                Some((block_number, block_hash)) => {
                    debug!("Returns a block from the base layer. Block number: {block_number}.");
                    yield SyncEvent::NewBaseLayerBlock { block_number, block_hash }
                }
                None => {
                    debug!(
                        "No blocks were proved on the base layer, waiting for blockchain to \
                         advance."
                    );
                }
            }
        }
    }
}

// This function is used to check if the sync is stuck.
// TODO(DvirYo): fix the bug and remove this function.
// TODO(dvir): add a test for this scenario.
fn check_sync_progress(
    reader: StorageReader,
    store_sierras_and_casms: bool,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        let mut txn=reader.begin_ro_txn()?;
        let mut header_marker=txn.get_header_marker()?;
        let mut state_marker=txn.get_state_marker()?;
        let mut casm_marker=txn.get_compiled_class_marker()?;
        loop{
            tokio::time::sleep(SLEEP_TIME_SYNC_PROGRESS).await;
            debug!("Checking if sync stopped progress.");
            txn=reader.begin_ro_txn()?;
            let new_header_marker=txn.get_header_marker()?;
            let new_state_marker=txn.get_state_marker()?;
            let new_casm_marker=txn.get_compiled_class_marker()?;
            let compiler_backward_compatibility_marker = txn.get_compiler_backward_compatibility_marker()?;
            let is_casm_stuck = casm_marker == new_casm_marker && (new_casm_marker < compiler_backward_compatibility_marker || store_sierras_and_casms);
            if header_marker==new_header_marker || state_marker==new_state_marker || is_casm_stuck {
                debug!("No progress in the sync. Return NoProgress event. Header marker: {header_marker}, \
                       State marker: {state_marker}, Casm marker: {casm_marker}.");
                yield SyncEvent::NoProgress;
            }
            header_marker=new_header_marker;
            state_marker=new_state_marker;
            casm_marker=new_casm_marker;
        }
    }
}
