// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![warn(missing_docs)]

//! A storage implementation for a [`Starknet`] node.
//!
//! This crate provides a writing and reading interface for various Starknet data structures to a
//! database. Enables at most one writing operation and multiple reading operations concurrently.
//! The underlying storage is implemented using the [`libmdbx`] crate.
//!
//! # Disclaimer
//! This crate is still under development and is not keeping backwards compatibility with previous
//! versions. Breaking changes are expected to happen in the near future.
//!
//! # Quick Start
//! To use this crate, open a storage by calling [`open_storage`] to get a [`StorageWriter`] and a
//! [`StorageReader`] and use them to create [`StorageTxn`] instances. The actual
//! functionality is implemented on the transaction in multiple traits.
//!
//! ```
//! use apollo_storage::open_storage;
//! # use apollo_storage::{db::DbConfig, StorageConfig};
//! use apollo_storage::header::{HeaderStorageReader, HeaderStorageWriter};    // Import the header API.
//! use starknet_api::block::{BlockHeader, BlockNumber, StarknetVersion};
//! use starknet_api::core::ChainId;
//!
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! let db_config = DbConfig {
//!     path_prefix: dir,
//!     chain_id: ChainId::Mainnet,
//!     enforce_file_exists: false,
//!     min_size: 1 << 20,    // 1MB
//!     max_size: 1 << 35,    // 32GB
//!     growth_step: 1 << 26, // 64MB
//! };
//! # let storage_config = StorageConfig{db_config, ..Default::default()};
//! let (reader, mut writer) = open_storage(storage_config)?;
//! writer
//!     .begin_rw_txn()?                                            // Start a RW transaction.
//!     .append_header(BlockNumber(0), &BlockHeader::default())?    // Append a header.
//!     .commit()?;                                                 // Commit the changes.
//!
//! let header = reader.begin_ro_txn()?.get_block_header(BlockNumber(0))?;  // Read the header.
//! assert_eq!(header, Some(BlockHeader::default()));
//! # Ok::<(), apollo_storage::StorageError>(())
//! ```
//! # Storage Version
//!
//! Attempting to open an existing database using a crate version with a mismatching storage version
//! will result in an error.
//!
//! The storage version is composed of two components: [`STORAGE_VERSION_STATE`] for the state and
//! [`STORAGE_VERSION_BLOCKS`] for blocks. Each version consists of a major and a minor version. A
//! higher major version indicates that a re-sync is necessary, while a higher minor version
//! indicates a change that is migratable.
//!
//! When a storage is opened with [`StorageScope::StateOnly`], only the state version must match.
//! For storage opened with [`StorageScope::FullArchive`], both versions must match the crate's
//! versions.
//!
//! Incompatibility occurs when the code and the database have differing major versions. However,
//! if the code has the same major version but a higher minor version compared to the database, it
//! will still function properly.
//!
//! Example cases:
//! - Code: {major: 0, minor: 0}, Database: {major: 1, minor: 0} will fail due to major version
//!   inequality.
//! - Code: {major: 0, minor: 0}, Database: {major: 0, minor: 1} will fail due to the smaller code's
//!   minor version.
//! - Code: {major: 0, minor: 1}, Database: {major: 0, minor: 0} will succeed since the major
//!   versions match and the code's minor version is higher.
//!
//! [`Starknet`]: https://starknet.io/
//! [`libmdbx`]: https://docs.rs/libmdbx/latest/libmdbx/

pub mod base_layer;
pub mod body;
pub mod class;
pub mod class_hash;
pub mod class_manager;
pub mod compiled_class;
#[cfg(feature = "document_calls")]
pub mod document_calls;
#[allow(missing_docs)]
pub mod metrics;
pub mod storage_metrics;
// TODO(yair): Make the compression_utils module pub(crate) or extract it from the crate.
#[doc(hidden)]
pub mod compression_utils;
pub mod db;
pub mod header;
pub mod mmap_file;
mod serialization;
pub mod state;
mod version;

mod deprecated;

#[cfg(test)]
mod test_instances;

#[cfg(any(feature = "testing", test))]
pub mod test_utils;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt::Debug;
use std::fs;
use std::sync::{Arc, Mutex};

use apollo_config::dumping::{SerializeConfig, prepend_sub_config_name, ser_param};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_proc_macros::{latency_histogram, sequencer_latency_histogram};
use body::events::EventIndex;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use db::db_stats::{DbTableStats, DbWholeStats};
use db::serialization::{Key, NoVersionValueWrapper, ValueSerde, VersionZeroWrapper};
use db::table_types::{CommonPrefix, NoValue, Table, TableType};
use mmap_file::{
    FileHandler,
    LocationInFile,
    MMapFileError,
    MmapFileConfig,
    Reader,
    Writer,
    open_file,
};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockSignature, StarknetVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{SierraContractClass, StateNumber, StorageKey, ThinStateDiff};
use starknet_api::transaction::{Transaction, TransactionHash, TransactionOutput};
use starknet_types_core::felt::Felt;
use tracing::{debug, info, warn};
use validator::Validate;
use version::{StorageVersionError, Version};

use crate::body::TransactionIndex;
use crate::db::table_types::SimpleTable;
use crate::db::{
    DbConfig,
    DbError,
    DbReader,
    DbTransaction,
    DbWriter,
    RO,
    RW,
    TableHandle,
    TableIdentifier,
    TransactionKind,
    open_env,
};
use crate::header::StorageBlockHeader;
use crate::metrics::{STORAGE_COMMIT_LATENCY, register_metrics};
use crate::mmap_file::MMapFileStats;
use crate::state::data::IndexedDeprecatedContractClass;
use crate::version::{VersionStorageReader, VersionStorageWriter};

// For more details on the storage version, see the module documentation.
/// The current version of the storage state code.
pub const STORAGE_VERSION_STATE: Version = Version { major: 6, minor: 0 };
/// The current version of the storage blocks code.
pub const STORAGE_VERSION_BLOCKS: Version = Version { major: 6, minor: 0 };

/// Opens a storage and returns a [`StorageReader`] and a [`StorageWriter`].
pub fn open_storage(
    storage_config: StorageConfig,
) -> StorageResult<(StorageReader, StorageWriter)> {
    info!("Opening storage: {}", storage_config.db_config.path_prefix.display());
    register_metrics();
    if !storage_config.db_config.path_prefix.exists()
        && !storage_config.db_config.enforce_file_exists
    {
        fs::create_dir_all(storage_config.db_config.path_prefix.clone())?;
        info!("Created storage directory: {}", storage_config.db_config.path_prefix.display());
    }

    let (db_reader, mut db_writer) = open_env(&storage_config.db_config)?;
    let tables = Arc::new(Tables {
        block_hash_to_number: db_writer.create_simple_table("block_hash_to_number")?,
        block_signatures: db_writer.create_simple_table("block_signatures")?,
        casms: db_writer.create_simple_table("casms")?,
        contract_storage: db_writer.create_common_prefix_table("contract_storage")?,
        declared_classes: db_writer.create_simple_table("declared_classes")?,
        declared_classes_block: db_writer.create_simple_table("declared_classes_block")?,
        deprecated_declared_classes: db_writer
            .create_simple_table("deprecated_declared_classes")?,
        deprecated_declared_classes_block: db_writer
            .create_simple_table("deprecated_declared_classes_block")?,
        deployed_contracts: db_writer.create_simple_table("deployed_contracts")?,
        events: db_writer.create_common_prefix_table("events")?,
        headers: db_writer.create_simple_table("headers")?,
        markers: db_writer.create_simple_table("markers")?,
        nonces: db_writer.create_common_prefix_table("nonces")?,
        file_offsets: db_writer.create_simple_table("file_offsets")?,
        state_diffs: db_writer.create_simple_table("state_diffs")?,
        transaction_hash_to_idx: db_writer.create_simple_table("transaction_hash_to_idx")?,
        transaction_metadata: db_writer.create_simple_table("transaction_metadata")?,

        // Version tables.
        starknet_version: db_writer.create_simple_table("starknet_version")?,
        storage_version: db_writer.create_simple_table("storage_version")?,

        // Class hashes.
        class_hash_to_executable_class_hash: db_writer
            .create_simple_table("class_hash_to_executable_class_hash")?,
    });
    let (file_writers, file_readers) = open_storage_files(
        &storage_config.db_config,
        storage_config.mmap_file_config,
        db_reader.clone(),
        &tables.file_offsets,
    )?;

    let reader = StorageReader {
        db_reader,
        tables: tables.clone(),
        scope: storage_config.scope,
        file_readers,
    };
    let batched_file_handlers = if storage_config.batch_config.enabled {
        Some(Arc::new(BatchedFileHandlers::new(
            file_writers.clone(),
            storage_config.batch_config.clone(),
        )))
    } else {
        None
    };

    let writer = StorageWriter {
        db_writer,
        tables,
        scope: storage_config.scope,
        file_writers,
        batch_config: storage_config.batch_config.clone(),
        batched_file_handlers,
    };

    let writer = set_version_if_needed(reader.clone(), writer)?;
    verify_storage_version(reader.clone())?;
    Ok((reader, writer))
}

// In case storage version does not exist, set it to the crate version.
// Expected to happen once - when the node is launched for the first time.
// If the storage scope has changed, update accordingly.
fn set_version_if_needed(
    reader: StorageReader,
    mut writer: StorageWriter,
) -> StorageResult<StorageWriter> {
    let Some(existing_storage_version) = get_storage_version(reader)? else {
        // Initialize the storage version.
        writer.begin_rw_txn()?.set_state_version(&STORAGE_VERSION_STATE)?.commit()?;
        // If in full-archive mode, also set the block version.
        if writer.scope == StorageScope::FullArchive {
            writer.begin_rw_txn()?.set_blocks_version(&STORAGE_VERSION_BLOCKS)?.commit()?;
        }
        debug!(
            "Storage was initialized with state_version: {:?}, scope: {:?}, blocks_version: {:?}",
            STORAGE_VERSION_STATE, writer.scope, STORAGE_VERSION_BLOCKS
        );
        return Ok(writer);
    };
    debug!("Existing storage state: {:?}", existing_storage_version);
    // Handle the case where the storage scope has changed.
    match existing_storage_version {
        StorageVersion::FullArchive(FullArchiveVersion { state_version: _, blocks_version: _ }) => {
            // TODO(yael): consider optimizing by deleting the block's data if the scope has changed
            // to StateOnly
            if writer.scope == StorageScope::StateOnly {
                // Deletion of the block's version is required here. It ensures that the node knows
                // that the storage operates in StateOnly mode and prevents the operator from
                // running it in FullArchive mode again.
                debug!("Changing the storage scope from FullArchive to StateOnly.");
                writer.begin_rw_txn()?.delete_blocks_version()?.commit()?;
            }
        }
        StorageVersion::StateOnly(StateOnlyVersion { state_version: _ }) => {
            // The storage cannot change from state-only to full-archive mode.
            if writer.scope == StorageScope::FullArchive {
                return Err(StorageError::StorageVersionInconsistency(
                    StorageVersionError::InconsistentStorageScope,
                ));
            }
        }
    }
    // Update the version if it's lower than the crate version.
    let mut wtxn = writer.begin_rw_txn()?;
    match existing_storage_version {
        StorageVersion::FullArchive(FullArchiveVersion { state_version, blocks_version }) => {
            // This allow is for when STORAGE_VERSION_STATE.minor = 0.
            #[allow(clippy::absurd_extreme_comparisons)]
            if STORAGE_VERSION_STATE.major == state_version.major
                && STORAGE_VERSION_STATE.minor > state_version.minor
            {
                debug!(
                    "Updating the storage state version from {:?} to {:?}",
                    state_version, STORAGE_VERSION_STATE
                );
                wtxn = wtxn.set_state_version(&STORAGE_VERSION_STATE)?;
            }
            // This allow is for when STORAGE_VERSION_BLOCKS.minor = 0.
            #[allow(clippy::absurd_extreme_comparisons)]
            if STORAGE_VERSION_BLOCKS.major == blocks_version.major
                && STORAGE_VERSION_BLOCKS.minor > blocks_version.minor
            {
                debug!(
                    "Updating the storage blocks version from {:?} to {:?}",
                    blocks_version, STORAGE_VERSION_BLOCKS
                );
                wtxn = wtxn.set_blocks_version(&STORAGE_VERSION_BLOCKS)?;
            }
        }
        StorageVersion::StateOnly(StateOnlyVersion { state_version }) => {
            // This allow is for when STORAGE_VERSION_STATE.minor = 0.
            #[allow(clippy::absurd_extreme_comparisons)]
            if STORAGE_VERSION_STATE.major == state_version.major
                && STORAGE_VERSION_STATE.minor > state_version.minor
            {
                debug!(
                    "Updating the storage state version from {:?} to {:?}",
                    state_version, STORAGE_VERSION_STATE
                );
                wtxn = wtxn.set_state_version(&STORAGE_VERSION_STATE)?;
            }
        }
    }
    wtxn.commit()?;
    Ok(writer)
}

#[derive(Debug)]
struct FullArchiveVersion {
    state_version: Version,
    blocks_version: Version,
}

#[derive(Debug)]
struct StateOnlyVersion {
    state_version: Version,
}

#[derive(Debug)]
enum StorageVersion {
    FullArchive(FullArchiveVersion),
    StateOnly(StateOnlyVersion),
}

fn get_storage_version(reader: StorageReader) -> StorageResult<Option<StorageVersion>> {
    let current_storage_version_state =
        reader.begin_ro_txn()?.get_state_version().map_err(|err| {
            if matches!(err, StorageError::InnerError(DbError::InnerDeserialization)) {
                tracing::error!(
                    "Cannot deserialize storage version. Storage major version has been changed, \
                     re-sync is needed."
                );
            }
            err
        })?;
    let current_storage_version_blocks = reader.begin_ro_txn()?.get_blocks_version()?;
    let Some(current_storage_version_state) = current_storage_version_state else {
        return Ok(None);
    };
    match current_storage_version_blocks {
        Some(current_storage_version_blocks) => {
            Ok(Some(StorageVersion::FullArchive(FullArchiveVersion {
                state_version: current_storage_version_state,
                blocks_version: current_storage_version_blocks,
            })))
        }
        None => Ok(Some(StorageVersion::StateOnly(StateOnlyVersion {
            state_version: current_storage_version_state,
        }))),
    }
}

// Assumes the storage has a version.
fn verify_storage_version(reader: StorageReader) -> StorageResult<()> {
    let existing_storage_version = get_storage_version(reader)?;
    debug!(
        "Crate storage version: State = {STORAGE_VERSION_STATE:} Blocks = \
         {STORAGE_VERSION_BLOCKS:}. Existing storage state: {existing_storage_version:?} "
    );

    match existing_storage_version {
        None => panic!("Storage should be initialized."),
        Some(StorageVersion::FullArchive(FullArchiveVersion {
            state_version: existing_state_version,
            blocks_version: _,
        })) if STORAGE_VERSION_STATE != existing_state_version => {
            Err(StorageError::StorageVersionInconsistency(
                StorageVersionError::InconsistentStorageVersion {
                    crate_version: STORAGE_VERSION_STATE,
                    storage_version: existing_state_version,
                },
            ))
        }

        Some(StorageVersion::FullArchive(FullArchiveVersion {
            state_version: _,
            blocks_version: existing_blocks_version,
        })) if STORAGE_VERSION_BLOCKS != existing_blocks_version => {
            Err(StorageError::StorageVersionInconsistency(
                StorageVersionError::InconsistentStorageVersion {
                    crate_version: STORAGE_VERSION_BLOCKS,
                    storage_version: existing_blocks_version,
                },
            ))
        }

        Some(StorageVersion::StateOnly(StateOnlyVersion {
            state_version: existing_state_version,
        })) if STORAGE_VERSION_STATE != existing_state_version => {
            Err(StorageError::StorageVersionInconsistency(
                StorageVersionError::InconsistentStorageVersion {
                    crate_version: STORAGE_VERSION_STATE,
                    storage_version: existing_state_version,
                },
            ))
        }
        Some(_) => Ok(()),
    }
}

/// The categories of data to save in the storage.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum StorageScope {
    /// Stores all types of data.
    #[default]
    FullArchive,
    /// Stores the data describing the current state. In this mode the transaction, events and
    /// state-diffs are not stored.
    StateOnly,
}

/// A struct for starting RO transactions ([`StorageTxn`]) to the storage.
#[derive(Clone)]
pub struct StorageReader {
    db_reader: DbReader,
    file_readers: FileHandlers<RO>,
    tables: Arc<Tables>,
    scope: StorageScope,
}

impl StorageReader {
    /// Takes a snapshot of the current state of the storage and returns a [`StorageTxn`] for
    /// reading data from the storage.
    pub fn begin_ro_txn(&self) -> StorageResult<StorageTxn<'_, RO>> {
        Ok(StorageTxn {
            txn: self.db_reader.begin_ro_txn()?,
            file_handlers: self.file_readers.clone(),
            batched_file_handlers: None, // Read-only doesn't need batching
            tables: self.tables.clone(),
            scope: self.scope,
        })
    }

    /// Returns metadata about the tables in the storage.
    pub fn db_tables_stats(&self) -> StorageResult<DbStats> {
        let mut tables_stats = BTreeMap::new();
        for name in Tables::field_names() {
            tables_stats.insert(name.to_string(), self.db_reader.get_table_stats(name)?);
        }
        Ok(DbStats { db_stats: self.db_reader.get_db_stats()?, tables_stats })
    }

    /// Returns metadata about the memory mapped files in the storage.
    pub fn mmap_files_stats(&self) -> HashMap<String, MMapFileStats> {
        self.file_readers.stats()
    }

    /// Returns the scope of the storage.
    pub fn get_scope(&self) -> StorageScope {
        self.scope
    }
}

/// A struct for starting RW transactions ([`StorageTxn`]) to the storage.
/// There is a single non clonable writer instance, to make sure there is only one write transaction
/// at any given moment.
pub struct StorageWriter {
    db_writer: DbWriter,
    file_writers: FileHandlers<RW>,
    tables: Arc<Tables>,
    scope: StorageScope,
    batch_config: BatchConfig,
    /// Shared batched file handlers that persist across transactions
    batched_file_handlers: Option<Arc<BatchedFileHandlers<RW>>>,
}

impl StorageWriter {
    /// Takes a snapshot of the current state of the storage and returns a [`StorageTxn`] for
    /// reading and modifying data in the storage.
    pub fn begin_rw_txn(&mut self) -> StorageResult<StorageTxn<'_, RW>> {
        Ok(StorageTxn {
            txn: self.db_writer.begin_rw_txn()?,
            file_handlers: self.file_writers.clone(),
            batched_file_handlers: self.batched_file_handlers.clone(),
            tables: self.tables.clone(),
            scope: self.scope,
        })
    }
}

/// A struct for interacting with the storage.
/// The actually functionality is implemented on the transaction in multiple traits.
pub struct StorageTxn<'env, Mode: TransactionKind> {
    txn: DbTransaction<'env, Mode>,
    file_handlers: FileHandlers<Mode>,
    batched_file_handlers: Option<Arc<BatchedFileHandlers<Mode>>>,
    tables: Arc<Tables>,
    scope: StorageScope,
}

impl StorageTxn<'_, RW> {
    /// Commits the changes made in the transaction to the storage.
    #[sequencer_latency_histogram(STORAGE_COMMIT_LATENCY, false)]
    pub fn commit(self) -> StorageResult<()> {
        let start = std::time::Instant::now();

        // If batching is enabled, only flush when we've accumulated enough blocks
        let should_commit_db = if let Some(ref batched_handlers) = self.batched_file_handlers {
            if batched_handlers.should_flush() {
                let (count, first_block) = batched_handlers.get_batch_info();
                info!(
                    "Batch size reached ({} blocks starting from {:?}), triggering flush",
                    count, first_block
                );
                let batch_flush_start = std::time::Instant::now();
                batched_handlers.flush_all_batches(&self.txn, &self.tables)?;
                info!("Batch flush completed in: {:?}", batch_flush_start.elapsed());
                true // We flushed to MDBX, so commit the transaction
            } else {
                let (count, first_block) = batched_handlers.get_batch_info();
                debug!(
                    "Batching enabled: {} blocks queued (starting from {:?}), not flushing yet \
                     (batch_size: {})",
                    count, first_block, batched_handlers.config.batch_size
                );
                false // Don't commit DB transaction, data is only in memory queues
            }
        } else {
            true // No batching, always commit
        };

        self.file_handlers.flush();
        let flush_time = start.elapsed();

        let db_start = std::time::Instant::now();
        if should_commit_db {
            self.txn.commit()?;
        }
        // If should_commit_db is false, we don't commit the transaction
        // The transaction will be automatically dropped/aborted, which is fine
        // because the data is still in memory queues waiting for the next flush
        let db_time = db_start.elapsed();

        debug!(
            "Storage commit completed - flush: {:?}, db_commit: {:?}, total: {:?}",
            flush_time,
            db_time,
            start.elapsed()
        );
        Ok(())
    }

    /// Helper to determine if batching is enabled
    pub(crate) fn is_batching_enabled(&self) -> bool {
        self.batched_file_handlers.is_some()
    }

    /// Helper to get batched file handlers (for internal use)
    pub(crate) fn batched_handlers(&self) -> Option<&BatchedFileHandlers<RW>> {
        self.batched_file_handlers.as_ref().map(|arc| arc.as_ref())
    }
}

impl<Mode: TransactionKind> StorageTxn<'_, Mode> {
    pub(crate) fn open_table<K: Key + Debug, V: ValueSerde + Debug, T: TableType>(
        &self,
        table_id: &TableIdentifier<K, V, T>,
    ) -> StorageResult<TableHandle<'_, K, V, T>> {
        if self.scope == StorageScope::StateOnly {
            let unused_tables = [
                self.tables.events.name,
                self.tables.transaction_hash_to_idx.name,
                self.tables.transaction_metadata.name,
            ];
            if unused_tables.contains(&table_id.name) {
                return Err(StorageError::ScopeError {
                    table_name: table_id.name.to_owned(),
                    storage_scope: self.scope,
                });
            }
        }
        Ok(self.txn.open_table(table_id)?)
    }
}

/// Returns the names of the tables in the storage.
pub fn table_names() -> &'static [&'static str] {
    Tables::field_names()
}

struct_field_names! {
    struct Tables {
        block_hash_to_number: TableIdentifier<BlockHash, NoVersionValueWrapper<BlockNumber>, SimpleTable>,
        block_signatures: TableIdentifier<BlockNumber, VersionZeroWrapper<BlockSignature>, SimpleTable>,
        casms: TableIdentifier<ClassHash, VersionZeroWrapper<LocationInFile>, SimpleTable>,
        // Empirically, defining the common prefix as (ContractAddress, StorageKey) is better space-wise than defining the
        // common prefix only as ContractAddress.
        contract_storage: TableIdentifier<((ContractAddress, StorageKey), BlockNumber), NoVersionValueWrapper<Felt>, CommonPrefix>,
        declared_classes: TableIdentifier<ClassHash, VersionZeroWrapper<LocationInFile>, SimpleTable>,
        declared_classes_block: TableIdentifier<ClassHash, NoVersionValueWrapper<BlockNumber>, SimpleTable>,
        deprecated_declared_classes: TableIdentifier<ClassHash, VersionZeroWrapper<IndexedDeprecatedContractClass>, SimpleTable>,
        deprecated_declared_classes_block: TableIdentifier<ClassHash, NoVersionValueWrapper<BlockNumber>, SimpleTable>,
        // TODO(dvir): consider use here also the CommonPrefix table type.
        deployed_contracts: TableIdentifier<(ContractAddress, BlockNumber), VersionZeroWrapper<ClassHash>, SimpleTable>,
        events: TableIdentifier<(ContractAddress, TransactionIndex), NoVersionValueWrapper<NoValue>, CommonPrefix>,
        headers: TableIdentifier<BlockNumber, VersionZeroWrapper<StorageBlockHeader>, SimpleTable>,
        markers: TableIdentifier<MarkerKind, VersionZeroWrapper<BlockNumber>, SimpleTable>,
        nonces: TableIdentifier<(ContractAddress, BlockNumber), VersionZeroWrapper<Nonce>, CommonPrefix>,
        file_offsets: TableIdentifier<OffsetKind, NoVersionValueWrapper<usize>, SimpleTable>,
        state_diffs: TableIdentifier<BlockNumber, VersionZeroWrapper<LocationInFile>, SimpleTable>,
        transaction_hash_to_idx: TableIdentifier<TransactionHash, NoVersionValueWrapper<TransactionIndex>, SimpleTable>,
        // TODO(dvir): consider not saving transaction hash and calculating it from the transaction on demand.
        transaction_metadata: TableIdentifier<TransactionIndex, VersionZeroWrapper<TransactionMetadata>, SimpleTable>,

        // Version tables
        starknet_version: TableIdentifier<BlockNumber, VersionZeroWrapper<StarknetVersion>, SimpleTable>,
        storage_version: TableIdentifier<String, NoVersionValueWrapper<Version>, SimpleTable>,

        // Class hashes.
        class_hash_to_executable_class_hash: TableIdentifier<ClassHash, NoVersionValueWrapper<CompiledClassHash>, SimpleTable>
    }
}

macro_rules! struct_field_names {
    (struct $name:ident { $($fname:ident : $ftype:ty),* }) => {
        pub(crate) struct $name {
            $($fname : $ftype),*
        }

        impl $name {
            fn field_names() -> &'static [&'static str] {
                static NAMES: &'static [&'static str] = &[$(stringify!($fname)),*];
                NAMES
            }
        }
    }
}
use struct_field_names;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransactionMetadata {
    tx_hash: TransactionHash,
    tx_location: LocationInFile,
    tx_output_location: LocationInFile,
}

// TODO(Yair): sort the variants alphabetically.
/// Error type for the storage crate.
#[allow(missing_docs)]
#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    /// Errors related to the underlying database.
    #[error(transparent)]
    InnerError(#[from] DbError),
    #[error("Marker mismatch (expected {expected}, found {found}).")]
    MarkerMismatch { expected: BlockNumber, found: BlockNumber },
    #[error(
        "State diff redefined a nonce {nonce:?} for contract {contract_address:?} at block \
         {block_number}."
    )]
    NonceReWrite { nonce: Nonce, block_number: BlockNumber, contract_address: ContractAddress },
    #[error(
        "Event with index {event_index:?} emitted from contract address {from_address:?} was not \
         found."
    )]
    EventNotFound { event_index: EventIndex, from_address: ContractAddress },
    #[error("DB in inconsistent state: {msg:?}.")]
    DBInconsistency { msg: String },
    /// Errors related to the underlying files.
    #[error(transparent)]
    MMapFileError(#[from] MMapFileError),
    #[error(transparent)]
    StorageVersionInconsistency(#[from] StorageVersionError),
    #[error("The table {table_name} is unused under the {storage_scope:?} storage scope.")]
    ScopeError { table_name: String, storage_scope: StorageScope },
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
    #[error(
        "The block number {block} should be smaller than the compiled_class_marker \
         {compiled_class_marker}."
    )]
    InvalidBlockNumber { block: BlockNumber, compiled_class_marker: BlockNumber },
    #[error(
        "Attempt to write block signature {block_signature:?} of non-existing block \
         {block_number}."
    )]
    BlockSignatureForNonExistingBlock { block_number: BlockNumber, block_signature: BlockSignature },
}

/// A type alias that maps to std::result::Result<T, StorageError>.
pub type StorageResult<V> = std::result::Result<V, StorageError>;

/// A struct for the configuration of the storage.
#[allow(missing_docs)]
#[derive(Serialize, Debug, Default, Deserialize, Clone, PartialEq, Validate)]
pub struct StorageConfig {
    #[validate]
    pub db_config: DbConfig,
    #[validate]
    pub mmap_file_config: MmapFileConfig,
    pub scope: StorageScope,
    /// Configuration for batching writes to improve performance
    #[serde(default)]
    pub batch_config: BatchConfig,
}

impl SerializeConfig for StorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dumped_config = BTreeMap::from_iter([ser_param(
            "scope",
            &self.scope,
            "The categories of data saved in storage.",
            ParamPrivacyInput::Public,
        )]);
        dumped_config
            .extend(prepend_sub_config_name(self.mmap_file_config.dump(), "mmap_file_config"));
        dumped_config.extend(prepend_sub_config_name(self.db_config.dump(), "db_config"));
        dumped_config.extend(prepend_sub_config_name(self.batch_config.dump(), "batch_config"));
        dumped_config
    }
}

/// A struct for the statistics of the tables in the database.
#[derive(Serialize, Deserialize, Debug)]
pub struct DbStats {
    /// Stats about the whole database.
    pub db_stats: DbWholeStats,
    /// A mapping from a table name in the database to its statistics.
    pub tables_stats: BTreeMap<String, DbTableStats>,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
// A marker is the first block number for which the corresponding data doesn't exist yet.
// Invariants:
// - CompiledClass <= Class <= State <= Header
// - Body <= Header
// - BaseLayerBlock <= Header
// Event is currently unsupported.
pub(crate) enum MarkerKind {
    Header,
    Body,
    Event,
    State,
    Class,
    CompiledClass,
    BaseLayerBlock,
    ClassManagerBlock,
    /// Marks the block beyond the last block that its classes can't be compiled with the current
    /// compiler version used in the class manager. Determined by starknet version.
    CompilerBackwardCompatibility,
}

pub(crate) type MarkersTable<'env> =
    TableHandle<'env, MarkerKind, VersionZeroWrapper<BlockNumber>, SimpleTable>;

#[derive(Clone, Debug)]
struct FileHandlers<Mode: TransactionKind> {
    thin_state_diff: FileHandler<VersionZeroWrapper<ThinStateDiff>, Mode>,
    contract_class: FileHandler<VersionZeroWrapper<SierraContractClass>, Mode>,
    casm: FileHandler<VersionZeroWrapper<CasmContractClass>, Mode>,
    deprecated_contract_class: FileHandler<VersionZeroWrapper<DeprecatedContractClass>, Mode>,
    transaction_output: FileHandler<VersionZeroWrapper<TransactionOutput>, Mode>,
    transaction: FileHandler<VersionZeroWrapper<Transaction>, Mode>,
}

/// Configuration for batching writes to improve performance
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct BatchConfig {
    /// Number of blocks to batch before writing to files
    pub batch_size: usize,
    /// Whether batching is enabled
    pub enabled: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self { batch_size: 100, enabled: true }
    }
}

impl SerializeConfig for BatchConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "batch_size",
                &self.batch_size,
                "Number of blocks to batch before writing to files.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "enabled",
                &self.enabled,
                "Whether batching is enabled for file writes.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

/// A single item in the batch queue
#[derive(Debug, Clone)]
struct BatchItem<T> {
    block_number: BlockNumber,
    data: T,
}

/// Pending MDBX write with metadata needed to execute it after flush
#[derive(Debug, Clone)]
struct PendingStateDiffWrite {
    block_number: BlockNumber,
    thin_state_diff: ThinStateDiff,
}

#[derive(Debug, Clone)]
struct PendingTransactionWrite {
    block_number: BlockNumber,
    index: usize,
    tx_hash: TransactionHash,
    transaction: Transaction,
    is_last: bool,
}

#[derive(Debug, Clone)]
struct PendingTransactionOutputWrite {
    block_number: BlockNumber,
    index: usize,
    transaction_output: TransactionOutput,
    is_last: bool,
}

#[derive(Debug, Clone)]
struct PendingContractClassWrite {
    class_hash: ClassHash,
    contract_class: SierraContractClass,
}

#[derive(Debug, Clone)]
struct PendingDeprecatedClassWrite {
    class_hash: ClassHash,
    block_number: BlockNumber,
    deprecated_class: DeprecatedContractClass,
}

#[derive(Debug, Clone)]
struct PendingCasmWrite {
    class_hash: ClassHash,
    casm: CasmContractClass,
}

/// Batched file handlers that accumulate writes before flushing to disk
#[derive(Debug)]
struct BatchedFileHandlers<Mode: TransactionKind> {
    /// The underlying file handlers
    file_handlers: FileHandlers<Mode>,
    /// Batch configuration
    config: BatchConfig,
    /// Counter for blocks queued since last flush
    blocks_queued: Arc<Mutex<usize>>,
    /// The first block number in the current batch (for logging)
    first_block_in_batch: Arc<Mutex<Option<BlockNumber>>>,
    /// Batched state diffs waiting to be written
    state_diff_batch: Arc<Mutex<VecDeque<BatchItem<ThinStateDiff>>>>,
    /// Batched transactions waiting to be written  
    transaction_batch: Arc<Mutex<VecDeque<BatchItem<Transaction>>>>,
    /// Batched transaction outputs waiting to be written
    transaction_output_batch: Arc<Mutex<VecDeque<BatchItem<TransactionOutput>>>>,
    /// Batched contract classes waiting to be written
    contract_class_batch: Arc<Mutex<VecDeque<BatchItem<SierraContractClass>>>>,
    /// Batched CASMs waiting to be written
    casm_batch: Arc<Mutex<VecDeque<BatchItem<CasmContractClass>>>>,
    /// Batched deprecated contract classes waiting to be written
    deprecated_class_batch: Arc<Mutex<VecDeque<BatchItem<DeprecatedContractClass>>>>,
    /// Pending state diff MDBX writes
    pending_state_diff_writes: Arc<Mutex<Vec<PendingStateDiffWrite>>>,
    /// Pending transaction MDBX writes
    pending_transaction_writes: Arc<Mutex<Vec<PendingTransactionWrite>>>,
    /// Pending transaction output MDBX writes
    pending_transaction_output_writes: Arc<Mutex<Vec<PendingTransactionOutputWrite>>>,
    /// Pending contract class MDBX writes
    pending_contract_class_writes: Arc<Mutex<Vec<PendingContractClassWrite>>>,
    /// Pending deprecated class MDBX writes
    pending_deprecated_class_writes: Arc<Mutex<Vec<PendingDeprecatedClassWrite>>>,
    /// Pending CASM MDBX writes
    pending_casm_writes: Arc<Mutex<Vec<PendingCasmWrite>>>,
}

impl<Mode: TransactionKind> BatchedFileHandlers<Mode> {
    /// Create a new batched file handler
    fn new(file_handlers: FileHandlers<Mode>, config: BatchConfig) -> Self {
        Self {
            file_handlers,
            config,
            blocks_queued: Arc::new(Mutex::new(0)),
            first_block_in_batch: Arc::new(Mutex::new(None)),
            state_diff_batch: Arc::new(Mutex::new(VecDeque::new())),
            transaction_batch: Arc::new(Mutex::new(VecDeque::new())),
            transaction_output_batch: Arc::new(Mutex::new(VecDeque::new())),
            contract_class_batch: Arc::new(Mutex::new(VecDeque::new())),
            casm_batch: Arc::new(Mutex::new(VecDeque::new())),
            deprecated_class_batch: Arc::new(Mutex::new(VecDeque::new())),
            pending_state_diff_writes: Arc::new(Mutex::new(Vec::new())),
            pending_transaction_writes: Arc::new(Mutex::new(Vec::new())),
            pending_transaction_output_writes: Arc::new(Mutex::new(Vec::new())),
            pending_contract_class_writes: Arc::new(Mutex::new(Vec::new())),
            pending_deprecated_class_writes: Arc::new(Mutex::new(Vec::new())),
            pending_casm_writes: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl BatchedFileHandlers<RW> {
    /// Queue a state diff for batched write (both file and MDBX)
    fn queue_state_diff(&self, block_number: BlockNumber, thin_state_diff: ThinStateDiff) {
        // Track this as a new block being queued
        {
            let mut first_block =
                self.first_block_in_batch.lock().expect("Lock should not be poisoned");
            if first_block.is_none() {
                *first_block = Some(block_number);
            }
        }
        {
            let mut counter = self.blocks_queued.lock().expect("Lock should not be poisoned");
            *counter += 1;
        }

        let mut batch = self.state_diff_batch.lock().expect("Lock should not be poisoned");
        batch.push_back(BatchItem { block_number, data: thin_state_diff.clone() });
        drop(batch);

        let mut pending =
            self.pending_state_diff_writes.lock().expect("Lock should not be poisoned");
        pending.push(PendingStateDiffWrite { block_number, thin_state_diff });
    }

    /// Queue a transaction for batched write (both file and MDBX)
    fn queue_transaction(
        &self,
        block_number: BlockNumber,
        index: usize,
        tx_hash: TransactionHash,
        transaction: Transaction,
        is_last: bool,
    ) {
        let mut batch = self.transaction_batch.lock().expect("Lock should not be poisoned");
        batch.push_back(BatchItem { block_number, data: transaction.clone() });
        drop(batch);

        let mut pending =
            self.pending_transaction_writes.lock().expect("Lock should not be poisoned");
        pending.push(PendingTransactionWrite {
            block_number,
            index,
            tx_hash,
            transaction,
            is_last,
        });
    }

    /// Queue a transaction output for batched write (both file and MDBX)
    fn queue_transaction_output(
        &self,
        block_number: BlockNumber,
        index: usize,
        transaction_output: TransactionOutput,
        is_last: bool,
    ) {
        let mut batch = self.transaction_output_batch.lock().expect("Lock should not be poisoned");
        batch.push_back(BatchItem { block_number, data: transaction_output.clone() });
        drop(batch);

        let mut pending =
            self.pending_transaction_output_writes.lock().expect("Lock should not be poisoned");
        pending.push(PendingTransactionOutputWrite {
            block_number,
            index,
            transaction_output,
            is_last,
        });
    }

    /// Queue a contract class for batched write (both file and MDBX)
    fn queue_contract_class(&self, class_hash: ClassHash, contract_class: SierraContractClass) {
        let mut batch = self.contract_class_batch.lock().expect("Lock should not be poisoned");
        batch.push_back(BatchItem {
            block_number: BlockNumber::default(),
            data: contract_class.clone(),
        });
        drop(batch);

        let mut pending =
            self.pending_contract_class_writes.lock().expect("Lock should not be poisoned");
        pending.push(PendingContractClassWrite { class_hash, contract_class });
    }

    /// Queue a deprecated contract class for batched write (both file and MDBX)
    fn queue_deprecated_contract_class(
        &self,
        class_hash: ClassHash,
        block_number: BlockNumber,
        deprecated_class: DeprecatedContractClass,
    ) {
        let mut batch = self.deprecated_class_batch.lock().expect("Lock should not be poisoned");
        batch.push_back(BatchItem { block_number, data: deprecated_class.clone() });
        drop(batch);

        let mut pending =
            self.pending_deprecated_class_writes.lock().expect("Lock should not be poisoned");
        pending.push(PendingDeprecatedClassWrite { class_hash, block_number, deprecated_class });
    }

    /// Queue a CASM for batched write (both file and MDBX)
    fn queue_casm(&self, class_hash: ClassHash, casm: CasmContractClass) {
        let mut batch = self.casm_batch.lock().expect("Lock should not be poisoned");
        batch.push_back(BatchItem { block_number: BlockNumber::default(), data: casm.clone() });
        drop(batch);

        let mut pending = self.pending_casm_writes.lock().expect("Lock should not be poisoned");
        pending.push(PendingCasmWrite { class_hash, casm });
    }

    /// Force flush all batches (used during shutdown or manual flush)
    fn flush_all_batches<'env>(
        &self,
        txn: &DbTransaction<'env, RW>,
        tables: &'env Tables,
    ) -> StorageResult<()> {
        info!("Starting batch flush with MDBX writes");

        // 1. Flush state diffs
        let mut state_diff_pending =
            self.pending_state_diff_writes.lock().expect("Lock should not be poisoned");
        if !state_diff_pending.is_empty() {
            let state_diffs_table = txn.open_table(&tables.state_diffs)?;
            let file_offset_table = txn.open_table(&tables.file_offsets)?;

            for pending in state_diff_pending.drain(..) {
                let location = self.file_handlers.append_state_diff(&pending.thin_state_diff);
                state_diffs_table.append(txn, &pending.block_number, &location)?;
                file_offset_table.upsert(
                    txn,
                    &OffsetKind::ThinStateDiff,
                    &location.next_offset(),
                )?;
            }
            info!("Flushed {} state diffs to files and MDBX", state_diff_pending.len());
        }
        drop(state_diff_pending);

        // 2. Flush transactions and transaction outputs together
        let mut tx_pending =
            self.pending_transaction_writes.lock().expect("Lock should not be poisoned");
        let mut tx_output_pending =
            self.pending_transaction_output_writes.lock().expect("Lock should not be poisoned");

        if !tx_pending.is_empty() || !tx_output_pending.is_empty() {
            let transaction_metadata_table = txn.open_table(&tables.transaction_metadata)?;
            let file_offset_table = txn.open_table(&tables.file_offsets)?;

            // Group by block_number and index, then write together
            for pending_tx in tx_pending.drain(..) {
                let tx_location = self.file_handlers.append_transaction(&pending_tx.transaction);
                let tx_offset_in_block =
                    starknet_api::transaction::TransactionOffsetInBlock(pending_tx.index);
                let transaction_index =
                    body::TransactionIndex(pending_tx.block_number, tx_offset_in_block);

                // Find corresponding output
                if let Some(pos) = tx_output_pending.iter().position(|out| {
                    out.block_number == pending_tx.block_number && out.index == pending_tx.index
                }) {
                    let pending_output = tx_output_pending.remove(pos);
                    let tx_output_location = self
                        .file_handlers
                        .append_transaction_output(&pending_output.transaction_output);

                    // Write metadata to MDBX
                    transaction_metadata_table.append(
                        txn,
                        &transaction_index,
                        &TransactionMetadata {
                            tx_location,
                            tx_output_location,
                            tx_hash: pending_tx.tx_hash,
                        },
                    )?;

                    // Update file offset table for last transaction
                    if pending_tx.is_last {
                        file_offset_table.upsert(
                            txn,
                            &OffsetKind::Transaction,
                            &tx_location.next_offset(),
                        )?;
                    }
                    if pending_output.is_last {
                        file_offset_table.upsert(
                            txn,
                            &OffsetKind::TransactionOutput,
                            &tx_output_location.next_offset(),
                        )?;
                    }
                }
            }

            info!("Flushed {} transactions and outputs to files and MDBX", tx_pending.len());
        }
        drop(tx_pending);
        drop(tx_output_pending);

        // 3. Flush contract classes
        let mut class_pending =
            self.pending_contract_class_writes.lock().expect("Lock should not be poisoned");
        if !class_pending.is_empty() {
            let declared_classes_table = txn.open_table(&tables.declared_classes)?;
            let file_offset_table = txn.open_table(&tables.file_offsets)?;

            for pending in class_pending.drain(..) {
                let location = self.file_handlers.append_contract_class(&pending.contract_class);
                declared_classes_table.insert(txn, &pending.class_hash, &location)?;
                file_offset_table.upsert(
                    txn,
                    &OffsetKind::ContractClass,
                    &location.next_offset(),
                )?;
            }
            info!("Flushed {} contract classes to files and MDBX", class_pending.len());
        }
        drop(class_pending);

        // 4. Flush deprecated contract classes
        let mut deprecated_pending =
            self.pending_deprecated_class_writes.lock().expect("Lock should not be poisoned");
        if !deprecated_pending.is_empty() {
            let deprecated_declared_classes_table =
                txn.open_table(&tables.deprecated_declared_classes)?;
            let file_offset_table = txn.open_table(&tables.file_offsets)?;

            for pending in deprecated_pending.drain(..) {
                let location =
                    self.file_handlers.append_deprecated_contract_class(&pending.deprecated_class);
                let value = state::data::IndexedDeprecatedContractClass {
                    block_number: pending.block_number,
                    location_in_file: location,
                };
                deprecated_declared_classes_table.insert(txn, &pending.class_hash, &value)?;
                file_offset_table.upsert(
                    txn,
                    &OffsetKind::DeprecatedContractClass,
                    &location.next_offset(),
                )?;
            }
            info!("Flushed {} deprecated classes to files and MDBX", deprecated_pending.len());
        }
        drop(deprecated_pending);

        // 5. Flush CASMs
        let mut casm_pending =
            self.pending_casm_writes.lock().expect("Lock should not be poisoned");
        if !casm_pending.is_empty() {
            let casm_table = txn.open_table(&tables.casms)?;
            let file_offset_table = txn.open_table(&tables.file_offsets)?;

            for pending in casm_pending.drain(..) {
                let location = self.file_handlers.append_casm(&pending.casm);
                casm_table.insert(txn, &pending.class_hash, &location)?;
                file_offset_table.upsert(txn, &OffsetKind::Casm, &location.next_offset())?;
            }
            info!("Flushed {} CASMs to files and MDBX", casm_pending.len());
        }
        drop(casm_pending);

        // After flushing all state diffs to MDBX, advance the compiled class marker
        // This was skipped during batched writes because the data wasn't in MDBX yet
        let state_diffs_table = txn.open_table(&tables.state_diffs)?;
        let markers_table = txn.open_table(&tables.markers)?;

        // Call the marker advancement function from state module
        // We need to make it accessible or recreate the logic here
        let state_marker = markers_table.get(txn, &MarkerKind::State)?.unwrap_or_default();
        let mut compiled_class_marker =
            markers_table.get(txn, &MarkerKind::CompiledClass)?.unwrap_or_default();

        while compiled_class_marker < state_marker {
            if let Some(state_diff_location) = state_diffs_table.get(txn, &compiled_class_marker)? {
                if let Ok(thin_state_diff) =
                    self.file_handlers.get_thin_state_diff_unchecked(state_diff_location)
                {
                    if !thin_state_diff.declared_classes.is_empty() {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
            compiled_class_marker = compiled_class_marker.unchecked_next();
            markers_table.upsert(txn, &MarkerKind::CompiledClass, &compiled_class_marker)?;
        }

        // Reset the counter and first block after successful flush
        {
            let mut counter = self.blocks_queued.lock().expect("Lock should not be poisoned");
            *counter = 0;
        }
        {
            let mut first_block =
                self.first_block_in_batch.lock().expect("Lock should not be poisoned");
            *first_block = None;
        }

        info!("Batch flush with MDBX writes completed successfully");
        Ok(())
    }

    /// Check if we should flush based on the number of blocks queued
    fn should_flush(&self) -> bool {
        let counter = self.blocks_queued.lock().expect("Lock should not be poisoned");
        *counter >= self.config.batch_size
    }

    /// Get the current batch info for logging
    fn get_batch_info(&self) -> (usize, Option<BlockNumber>) {
        let counter = self.blocks_queued.lock().expect("Lock should not be poisoned");
        let first_block = self.first_block_in_batch.lock().expect("Lock should not be poisoned");
        (*counter, *first_block)
    }
}

impl FileHandlers<RW> {
    // Appends a thin state diff to the corresponding file and returns its location.
    #[latency_histogram("storage_file_handler_append_state_diff_latency_seconds", true)]
    fn append_state_diff(&self, thin_state_diff: &ThinStateDiff) -> LocationInFile {
        self.clone().thin_state_diff.append(thin_state_diff)
    }

    // Appends a contract class to the corresponding file and returns its location.
    fn append_contract_class(&self, contract_class: &SierraContractClass) -> LocationInFile {
        self.clone().contract_class.append(contract_class)
    }

    // Appends a CASM to the corresponding file and returns its location.
    fn append_casm(&self, casm: &CasmContractClass) -> LocationInFile {
        self.clone().casm.append(casm)
    }

    // Appends a deprecated contract class to the corresponding file and returns its location.
    fn append_deprecated_contract_class(
        &self,
        deprecated_contract_class: &DeprecatedContractClass,
    ) -> LocationInFile {
        self.clone().deprecated_contract_class.append(deprecated_contract_class)
    }

    // Appends a thin transaction output to the corresponding file and returns its location.
    fn append_transaction_output(&self, transaction_output: &TransactionOutput) -> LocationInFile {
        self.clone().transaction_output.append(transaction_output)
    }

    // Appends a transaction to the corresponding file and returns its location.
    fn append_transaction(&self, transaction: &Transaction) -> LocationInFile {
        self.clone().transaction.append(transaction)
    }

    // TODO(dan): Consider 1. flushing only the relevant files, 2. flushing concurrently.
    #[latency_histogram("storage_file_handler_flush_latency_seconds", false)]
    fn flush(&self) {
        let start = std::time::Instant::now();
        debug!("Starting sequential flush of all file handlers");

        let flush_start = std::time::Instant::now();
        self.thin_state_diff.flush();
        debug!("Flushed thin_state_diff in {:?}", flush_start.elapsed());

        let flush_start = std::time::Instant::now();
        self.contract_class.flush();
        debug!("Flushed contract_class in {:?}", flush_start.elapsed());

        let flush_start = std::time::Instant::now();
        self.casm.flush();
        debug!("Flushed casm in {:?}", flush_start.elapsed());

        let flush_start = std::time::Instant::now();
        self.deprecated_contract_class.flush();
        debug!("Flushed deprecated_contract_class in {:?}", flush_start.elapsed());

        let flush_start = std::time::Instant::now();
        self.transaction_output.flush();
        debug!("Flushed transaction_output in {:?}", flush_start.elapsed());

        let flush_start = std::time::Instant::now();
        self.transaction.flush();
        debug!("Flushed transaction in {:?}", flush_start.elapsed());

        debug!("Sequential flush completed in total time: {:?}", start.elapsed());
    }

    #[allow(dead_code)]
    fn flush_concurrent(&self) {
        debug!("Flushing the mmap files concurrently using threads.");
        let thin_state_diff = self.thin_state_diff.clone();
        let contract_class = self.contract_class.clone();
        let casm = self.casm.clone();
        let deprecated_contract_class = self.deprecated_contract_class.clone();
        let transaction_output = self.transaction_output.clone();
        let transaction = self.transaction.clone();
        let handles = vec![
            std::thread::spawn(move || thin_state_diff.flush()),
            std::thread::spawn(move || contract_class.flush()),
            std::thread::spawn(move || casm.flush()),
            std::thread::spawn(move || deprecated_contract_class.flush()),
            std::thread::spawn(move || transaction_output.flush()),
            std::thread::spawn(move || transaction.flush()),
        ];
        for (i, handle) in handles.into_iter().enumerate() {
            if let Err(_) = handle.join() {
                warn!("Flush thread {} panicked during concurrent flush", i);
            }
        }
        debug!("All concurrent flush operations completed.");
    }
}

impl<Mode: TransactionKind> FileHandlers<Mode> {
    pub fn stats(&self) -> HashMap<String, MMapFileStats> {
        // TODO(Yair): use consts for the file names.
        HashMap::from_iter([
            ("thin_state_diff".to_string(), self.thin_state_diff.stats()),
            ("contract_class".to_string(), self.contract_class.stats()),
            ("casm".to_string(), self.casm.stats()),
            ("deprecated_contract_class".to_string(), self.deprecated_contract_class.stats()),
            ("transaction_output".to_string(), self.transaction_output.stats()),
            ("transaction".to_string(), self.transaction.stats()),
        ])
    }

    // Returns the thin state diff at the given location or an error in case it doesn't exist.
    fn get_thin_state_diff_unchecked(
        &self,
        location: LocationInFile,
    ) -> StorageResult<ThinStateDiff> {
        self.thin_state_diff.get(location)?.ok_or(StorageError::DBInconsistency {
            msg: format!("ThinStateDiff at location {:?} not found.", location),
        })
    }

    // Returns the contract class at the given location or an error in case it doesn't exist.
    fn get_contract_class_unchecked(
        &self,
        location: LocationInFile,
    ) -> StorageResult<SierraContractClass> {
        self.contract_class.get(location)?.ok_or(StorageError::DBInconsistency {
            msg: format!("ContractClass at location {:?} not found.", location),
        })
    }

    // Returns the CASM at the given location or an error in case it doesn't exist.
    fn get_casm_unchecked(&self, location: LocationInFile) -> StorageResult<CasmContractClass> {
        self.casm.get(location)?.ok_or(StorageError::DBInconsistency {
            msg: format!("CasmContractClass at location {:?} not found.", location),
        })
    }

    // Returns the deprecated contract class at the given location or an error in case it doesn't
    // exist.
    fn get_deprecated_contract_class_unchecked(
        &self,
        location: LocationInFile,
    ) -> StorageResult<DeprecatedContractClass> {
        self.deprecated_contract_class.get(location)?.ok_or(StorageError::DBInconsistency {
            msg: format!("DeprecatedContractClass at location {:?} not found.", location),
        })
    }

    // Returns the transaction output at the given location or an error in case it doesn't
    // exist.
    fn get_transaction_output_unchecked(
        &self,
        location: LocationInFile,
    ) -> StorageResult<TransactionOutput> {
        self.transaction_output.get(location)?.ok_or(StorageError::DBInconsistency {
            msg: format!("TransactionOutput at location {:?} not found.", location),
        })
    }

    // Returns the transaction at the given location or an error in case it doesn't exist.
    fn get_transaction_unchecked(&self, location: LocationInFile) -> StorageResult<Transaction> {
        self.transaction.get(location)?.ok_or(StorageError::DBInconsistency {
            msg: format!("Transaction at location {:?} not found.", location),
        })
    }
}

fn open_storage_files(
    db_config: &DbConfig,
    mmap_file_config: MmapFileConfig,
    db_reader: DbReader,
    file_offsets_table: &TableIdentifier<OffsetKind, NoVersionValueWrapper<usize>, SimpleTable>,
) -> StorageResult<(FileHandlers<RW>, FileHandlers<RO>)> {
    let db_transaction = db_reader.begin_ro_txn()?;
    let table = db_transaction.open_table(file_offsets_table)?;

    // TODO(dvir): consider using a loop here to avoid code duplication.
    let thin_state_diff_offset =
        table.get(&db_transaction, &OffsetKind::ThinStateDiff)?.unwrap_or_default();
    let (thin_state_diff_writer, thin_state_diff_reader) = open_file(
        mmap_file_config.clone(),
        db_config.path().join("thin_state_diff.dat"),
        thin_state_diff_offset,
    )?;

    let contract_class_offset =
        table.get(&db_transaction, &OffsetKind::ContractClass)?.unwrap_or_default();
    let (contract_class_writer, contract_class_reader) = open_file(
        mmap_file_config.clone(),
        db_config.path().join("contract_class.dat"),
        contract_class_offset,
    )?;

    let casm_offset = table.get(&db_transaction, &OffsetKind::Casm)?.unwrap_or_default();
    let (casm_writer, casm_reader) =
        open_file(mmap_file_config.clone(), db_config.path().join("casm.dat"), casm_offset)?;

    let deprecated_contract_class_offset =
        table.get(&db_transaction, &OffsetKind::DeprecatedContractClass)?.unwrap_or_default();
    let (deprecated_contract_class_writer, deprecated_contract_class_reader) = open_file(
        mmap_file_config.clone(),
        db_config.path().join("deprecated_contract_class.dat"),
        deprecated_contract_class_offset,
    )?;

    let transaction_output_offset =
        table.get(&db_transaction, &OffsetKind::TransactionOutput)?.unwrap_or_default();
    let (transaction_output_writer, transaction_output_reader) = open_file(
        mmap_file_config.clone(),
        db_config.path().join("transaction_output.dat"),
        transaction_output_offset,
    )?;

    let transaction_offset =
        table.get(&db_transaction, &OffsetKind::Transaction)?.unwrap_or_default();
    let (transaction_writer, transaction_reader) =
        open_file(mmap_file_config, db_config.path().join("transaction.dat"), transaction_offset)?;
    // the files
    Ok((
        FileHandlers {
            thin_state_diff: thin_state_diff_writer,
            contract_class: contract_class_writer,
            casm: casm_writer,
            deprecated_contract_class: deprecated_contract_class_writer,
            transaction_output: transaction_output_writer,
            transaction: transaction_writer,
        },
        FileHandlers {
            thin_state_diff: thin_state_diff_reader,
            contract_class: contract_class_reader,
            casm: casm_reader,
            deprecated_contract_class: deprecated_contract_class_reader,
            transaction_output: transaction_output_reader,
            transaction: transaction_reader,
        },
    ))
}

/// Represents a kind of mmap file.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
pub enum OffsetKind {
    /// A thin state diff file.
    ThinStateDiff,
    /// A contract class file.
    ContractClass,
    /// A CASM file.
    Casm,
    /// A deprecated contract class file.
    DeprecatedContractClass,
    /// A transaction output file.
    TransactionOutput,
    /// A transaction file.
    Transaction,
}

/// A storage query. Used for benchmarking in the storage_benchmark binary.
// TODO(dvir): add more queries (especially get casm).
// TODO(dvir): consider move this, maybe to test_utils.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageQuery {
    /// Get the class hash at a given state number.
    GetClassHashAt(StateNumber, ContractAddress),
    /// Get the nonce at a given state number.
    GetNonceAt(StateNumber, ContractAddress),
    /// Get the storage at a given state number.
    GetStorageAt(StateNumber, ContractAddress, StorageKey),
}
