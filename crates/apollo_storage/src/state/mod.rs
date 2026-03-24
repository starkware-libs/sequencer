//! Interface for handling data related to Starknet [state](https://docs.rs/starknet_api/latest/starknet_api/state/index.html).
//!
//! Import [`StateStorageReader`] and [`StateStorageWriter`] to read and write data related to state
//! diffs using a [`StorageTxn`].
//!
//! See [`StateReader`] struct for querying specific data from the state.
//!
//! # Example
//! ```
//! use apollo_storage::open_storage;
//! use apollo_storage::state::{StateStorageReader, StateStorageWriter};
//! # use indexmap::IndexMap;
//! # use apollo_storage::{db::DbConfig, StorageConfig};
//! # use starknet_api::block::BlockNumber;
//! # use starknet_api::core::{ChainId, ContractAddress};
//! use starknet_api::state::{StateNumber, ThinStateDiff};
//!
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! # let db_config = DbConfig {
//! #     path_prefix: dir,
//! #     chain_id: ChainId::Mainnet,
//! #     enforce_file_exists: false,
//! #     min_size: 1 << 20,    // 1MB
//! #     max_size: 1 << 35,    // 32GB
//! #     growth_step: 1 << 26, // 64MB
//! #     max_readers: 1 << 13, // 8K readers
//! # };
//! # let storage_config = StorageConfig{db_config, ..Default::default()};
//! let state_diff = ThinStateDiff::default();
//! let (reader, mut writer) = open_storage(storage_config)?;
//! writer
//!     .begin_rw_txn()? // Start a RW transaction.
//!     .append_state_diff(BlockNumber(0), state_diff.clone())? // Append a state diff.
//!     .commit()?;
//!
//! // Get the state diff.
//! let read_state_diff = reader.begin_ro_txn()?.get_state_diff(BlockNumber(0))?;
//! assert_eq!(read_state_diff, Some(state_diff));
//!
//! # let contract_address = ContractAddress::default();
//! // Get the class hash of a contract at a given state number.
//! // The transaction must live at least as long as the state reader.
//! let txn = reader.begin_ro_txn()?;
//! let state_reader = txn.get_state_reader()?;
//! let class_hash = state_reader.get_class_hash_at(
//!     StateNumber::unchecked_right_after_block(BlockNumber(0)),
//!     &contract_address,
//! )?;
//! # Ok::<(), apollo_storage::StorageError>(())
//! ```

#[doc(hidden)]
pub mod data;
#[cfg(test)]
mod state_test;

use std::collections::HashSet;

use apollo_proc_macros::{latency_histogram, sequencer_latency_histogram};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{SierraContractClass, StateNumber, StorageKey, ThinStateDiff};
use starknet_types_core::felt::Felt;
use tracing::debug;

use crate::db::serialization::{ChangesetValueWrapper, NoVersionValueWrapper, VersionZeroWrapper};
use crate::db::table_types::{CommonPrefix, DbCursorTrait, SimpleTable, Table};
use crate::db::{DbTransaction, TableHandle, TransactionKind, RW};
use crate::metrics::{
    BATCHER_CHANGESET_MARKER,
    BATCHER_CHANGESET_PRUNED_MARKER,
    STORAGE_APPEND_THIN_STATE_DIFF_LATENCY,
};
use crate::mmap_file::LocationInFile;
use crate::state::data::IndexedDeprecatedContractClass;
use crate::{
    FileHandlers,
    MarkerKind,
    MarkersTable,
    OffsetKind,
    StorageError,
    StorageResult,
    StorageTxn,
};

// TODO(shahak): Move the table aliases to the crate level.
pub(crate) type FileOffsetTable<'env> =
    TableHandle<'env, OffsetKind, NoVersionValueWrapper<usize>, SimpleTable>;
pub(crate) type DeclaredClassesTable<'env> =
    TableHandle<'env, ClassHash, VersionZeroWrapper<LocationInFile>, SimpleTable>;
pub(crate) type DeclaredClassesBlockTable<'env> =
    TableHandle<'env, ClassHash, NoVersionValueWrapper<BlockNumber>, SimpleTable>;
pub(crate) type DeprecatedDeclaredClassesTable<'env> =
    TableHandle<'env, ClassHash, VersionZeroWrapper<IndexedDeprecatedContractClass>, SimpleTable>;
pub(crate) type DeprecatedDeclaredClassesBlockTable<'env> =
    TableHandle<'env, ClassHash, NoVersionValueWrapper<BlockNumber>, SimpleTable>;
pub(crate) type CompiledClassesTable<'env> =
    TableHandle<'env, ClassHash, VersionZeroWrapper<LocationInFile>, SimpleTable>;
pub(crate) type StatelessCompiledClassHashV2Table<'env> =
    TableHandle<'env, ClassHash, NoVersionValueWrapper<CompiledClassHash>, SimpleTable>;
pub(crate) type DeployedContractsTable<'env> =
    TableHandle<'env, (ContractAddress, BlockNumber), VersionZeroWrapper<ClassHash>, SimpleTable>;
pub(crate) type ContractStorageTable<'env> = TableHandle<
    'env,
    ((ContractAddress, StorageKey), BlockNumber),
    NoVersionValueWrapper<Felt>,
    CommonPrefix,
>;
pub(crate) type NoncesTable<'env> =
    TableHandle<'env, (ContractAddress, BlockNumber), VersionZeroWrapper<Nonce>, CommonPrefix>;
pub(crate) type CompiledClassHashTable<'env> = TableHandle<
    'env,
    (ClassHash, BlockNumber),
    VersionZeroWrapper<CompiledClassHash>,
    CommonPrefix,
>;

// Flat state table types (no BlockNumber in key).
pub(crate) type FlatContractStorageTable<'env> =
    TableHandle<'env, (ContractAddress, StorageKey), NoVersionValueWrapper<Felt>, CommonPrefix>;
pub(crate) type FlatNoncesTable<'env> =
    TableHandle<'env, ContractAddress, NoVersionValueWrapper<Nonce>, SimpleTable>;
pub(crate) type FlatDeployedContractsTable<'env> =
    TableHandle<'env, ContractAddress, NoVersionValueWrapper<ClassHash>, SimpleTable>;
pub(crate) type FlatCompiledClassHashTable<'env> =
    TableHandle<'env, ClassHash, NoVersionValueWrapper<CompiledClassHash>, SimpleTable>;

// Changeset table types (store pre-images before each block's changes).
pub(crate) type ChangesetContractStorageTable<'env> = TableHandle<
    'env,
    ((BlockNumber, ContractAddress), StorageKey),
    ChangesetValueWrapper<NoVersionValueWrapper<Felt>>,
    CommonPrefix,
>;
pub(crate) type ChangesetNoncesTable<'env> = TableHandle<
    'env,
    (BlockNumber, ContractAddress),
    ChangesetValueWrapper<NoVersionValueWrapper<Nonce>>,
    CommonPrefix,
>;
pub(crate) type ChangesetDeployedContractsTable<'env> = TableHandle<
    'env,
    (BlockNumber, ContractAddress),
    ChangesetValueWrapper<NoVersionValueWrapper<ClassHash>>,
    CommonPrefix,
>;
pub(crate) type ChangesetCompiledClassHashTable<'env> = TableHandle<
    'env,
    (BlockNumber, ClassHash),
    ChangesetValueWrapper<NoVersionValueWrapper<CompiledClassHash>>,
    CommonPrefix,
>;

pub(crate) struct ChangesetTables<'env> {
    pub deployed: ChangesetDeployedContractsTable<'env>,
    pub nonces: ChangesetNoncesTable<'env>,
    pub storage: ChangesetContractStorageTable<'env>,
    pub compiled: ChangesetCompiledClassHashTable<'env>,
}

/// Interface for reading data related to the state.
// Structure of state data:
// * declared_classes_table: (class_hash) -> (block_num, contract_class). Each entry specifies at
//   which block was this class declared and with what class definition. For Cairo 1 class
//   definitions.
// * deprecated_declared_classes_table: (class_hash) -> (block_num, deprecated_contract_class). Each
//   entry specifies at which block was this class declared and with what class definition. For
//   Cairo 0 class definitions.
// * deployed_contracts_table: (contract_address, block_num) -> (class_hash). Each entry specifies
//   at which block was this contract deployed (or its class got replaced) and with what class hash.
// * storage_table: (contract_address, key, block_num) -> (value). Specifies that at `block_num`,
//   the `key` at `contract_address` was changed to `value`. This structure let's us do quick
//   lookup, since the database supports "Get the closet element from  the left". Thus, to lookup
//   the value at a specific block_number, we can search (contract_address, key, block_num), and
//   retrieve the closest from left, which should be the latest update to the value before that
//   block_num.
// * nonces_table: (contract_address, block_num) -> (nonce). Specifies that at `block_num`, the
//   nonce of `contract_address` was changed to `nonce`.
// * compiled_class_hash_table: (class_hash, block_num) -> (compiled_class_hash). Specifies that at
//   `block_num`, the compiled class hash of `class_hash` was changed to `compiled_class_hash`.
pub trait StateStorageReader<Mode: TransactionKind> {
    /// The state marker is the first block number that doesn't exist yet.
    fn get_state_marker(&self) -> StorageResult<BlockNumber>;
    /// The changeset marker is the first block number that doesn't have changeset pre-images yet.
    fn get_changeset_marker(&self) -> StorageResult<BlockNumber>;
    /// The changeset pruned marker is the oldest block that still has changeset entries.
    fn get_changeset_pruned_marker(&self) -> StorageResult<BlockNumber>;
    /// Returns the state diff at a given block number.
    fn get_state_diff(&self, block_number: BlockNumber) -> StorageResult<Option<ThinStateDiff>>;
    /// Returns a state reader.
    fn get_state_reader(&self) -> StorageResult<StateReader<'_, Mode>>;
}

type RevertedStateDiff = (
    ThinStateDiff,
    Vec<ClassHash>,
    IndexMap<ClassHash, SierraContractClass>,
    Vec<ClassHash>,
    IndexMap<ClassHash, DeprecatedContractClass>,
    IndexMap<ClassHash, CasmContractClass>,
);

/// Interface for writing data related to the state.
pub trait StateStorageWriter
where
    Self: Sized,
{
    /// Appends a state diff without classes to the storage.
    fn append_state_diff(
        self,
        block_number: BlockNumber,
        thin_state_diff: ThinStateDiff,
    ) -> StorageResult<Self>;

    /// Removes a state diff from the storage and returns the removed data.
    fn revert_state_diff(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<RevertedStateDiff>)>;
}

impl<Mode: TransactionKind> StateStorageReader<Mode> for StorageTxn<'_, Mode> {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_state_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::State)?.unwrap_or_default())
    }
    fn get_changeset_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::Changeset)?.unwrap_or_default())
    }
    fn get_changeset_pruned_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::ChangesetPruned)?.unwrap_or_default())
    }
    fn get_state_diff(&self, block_number: BlockNumber) -> StorageResult<Option<ThinStateDiff>> {
        let state_diff_location = self.get_state_diff_location(block_number)?;
        match state_diff_location {
            None => Ok(None),
            Some(location) => Ok(Some(self.get_state_diff_from_location(location)?)),
        }
    }

    fn get_state_reader(&self) -> StorageResult<StateReader<'_, Mode>> {
        StateReader::new(self)
    }
}

impl<'env, Mode: TransactionKind> StorageTxn<'env, Mode> {
    /// Reads changeset tables for a given block and returns a `ThinStateDiff` containing the
    /// pre-image values (the state *before* this block was applied). `None` pre-images (meaning
    /// the key did not exist before) are converted to type defaults.
    pub fn get_reversed_state_diff_from_changeset(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<ThinStateDiff> {
        // Verify the requested block has changeset data. Without this check, a non-flat-state
        // node (which never writes changesets) would silently return an empty diff.
        let changeset_marker = self.get_changeset_marker()?;
        if block_number >= changeset_marker {
            return Err(StorageError::DBInconsistency {
                msg: format!(
                    "Requested reversed state diff from changeset for block {block_number}, but \
                     changeset marker is only at {changeset_marker}."
                ),
            });
        }

        let changeset_pruned_marker = self.get_changeset_pruned_marker()?;
        if block_number < changeset_pruned_marker {
            return Err(StorageError::RevertBeyondChangesetHistory {
                block_number,
                oldest_changeset: changeset_pruned_marker,
            });
        }

        let changeset_deployed_table =
            self.open_table(&self.tables.changeset_deployed_contracts)?;
        let changeset_nonces_table = self.open_table(&self.tables.changeset_nonces)?;
        let changeset_storage_table = self.open_table(&self.tables.changeset_contract_storage)?;
        let changeset_compiled_table =
            self.open_table(&self.tables.changeset_compiled_class_hash)?;

        // Deployed contracts.
        let mut deployed_contracts = IndexMap::new();
        {
            let mut cursor = changeset_deployed_table.cursor(&self.txn)?;
            let mut entry = cursor.lower_bound(&(block_number, ContractAddress::default()))?;
            while let Some(((got_block, address), preimage)) = entry {
                if got_block != block_number {
                    break;
                }
                deployed_contracts.insert(address, preimage.unwrap_or_default());
                entry = cursor.next()?;
            }
        }

        // Nonces.
        let mut nonces = IndexMap::new();
        {
            let mut cursor = changeset_nonces_table.cursor(&self.txn)?;
            let mut entry = cursor.lower_bound(&(block_number, ContractAddress::default()))?;
            while let Some(((got_block, address), preimage)) = entry {
                if got_block != block_number {
                    break;
                }
                nonces.insert(address, preimage.unwrap_or_default());
                entry = cursor.next()?;
            }
        }

        // Storage diffs.
        let mut storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, Felt>> =
            IndexMap::new();
        {
            let mut cursor = changeset_storage_table.cursor(&self.txn)?;
            let mut entry = cursor.lower_bound(&(
                (block_number, ContractAddress::default()),
                StorageKey::default(),
            ))?;
            while let Some((((got_block, address), key), preimage)) = entry {
                if got_block != block_number {
                    break;
                }
                storage_diffs.entry(address).or_default().insert(key, preimage.unwrap_or_default());
                entry = cursor.next()?;
            }
        }

        // Compiled class hashes.
        let mut class_hash_to_compiled_class_hash = IndexMap::new();
        {
            let mut cursor = changeset_compiled_table.cursor(&self.txn)?;
            let mut entry = cursor.lower_bound(&(block_number, ClassHash::default()))?;
            while let Some(((got_block, hash), preimage)) = entry {
                if got_block != block_number {
                    break;
                }
                class_hash_to_compiled_class_hash.insert(hash, preimage.unwrap_or_default());
                entry = cursor.next()?;
            }
        }

        Ok(ThinStateDiff {
            deployed_contracts,
            nonces,
            storage_diffs,
            class_hash_to_compiled_class_hash,
            deprecated_declared_classes: Vec::new(),
        })
    }
}

/// A single coherent state at a single point in time,
pub struct StateReader<'env, Mode: TransactionKind> {
    txn: &'env DbTransaction<'env, Mode>,
    declared_classes_table: DeclaredClassesTable<'env>,
    declared_classes_block_table: DeclaredClassesBlockTable<'env>,
    deprecated_declared_classes_table: DeprecatedDeclaredClassesTable<'env>,
    deprecated_declared_classes_block_table: DeprecatedDeclaredClassesBlockTable<'env>,
    deployed_contracts_table: DeployedContractsTable<'env>,
    nonces_table: NoncesTable<'env>,
    compiled_class_hash_table: CompiledClassHashTable<'env>,
    storage_table: ContractStorageTable<'env>,
    markers_table: MarkersTable<'env>,
    file_handlers: &'env FileHandlers<Mode>,
    // Flat state fields.
    flat_state: bool,
    flat_contract_storage_table: FlatContractStorageTable<'env>,
    flat_nonces_table: FlatNoncesTable<'env>,
    flat_deployed_contracts_table: FlatDeployedContractsTable<'env>,
    flat_compiled_class_hash_table: FlatCompiledClassHashTable<'env>,
}

impl<'env, Mode: TransactionKind> StateReader<'env, Mode> {
    /// Creates a new state reader from a storage transaction.
    ///
    /// Opens a handle to each table to be used when reading.
    ///
    /// # Arguments
    /// * txn - A storage transaction object.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error opening the tables.
    fn new(txn: &'env StorageTxn<'env, Mode>) -> StorageResult<Self> {
        let compiled_class_hash_table = txn.txn.open_table(&txn.tables.compiled_class_hash)?;
        let declared_classes_table = txn.txn.open_table(&txn.tables.declared_classes)?;
        let declared_classes_block_table =
            txn.txn.open_table(&txn.tables.declared_classes_block)?;
        let deprecated_declared_classes_table =
            txn.txn.open_table(&txn.tables.deprecated_declared_classes)?;
        let deprecated_declared_classes_block_table =
            txn.txn.open_table(&txn.tables.deprecated_declared_classes_block)?;
        let deployed_contracts_table = txn.txn.open_table(&txn.tables.deployed_contracts)?;
        let nonces_table = txn.txn.open_table(&txn.tables.nonces)?;
        let storage_table = txn.txn.open_table(&txn.tables.contract_storage)?;
        let markers_table = txn.txn.open_table(&txn.tables.markers)?;
        let flat_contract_storage_table = txn.txn.open_table(&txn.tables.flat_contract_storage)?;
        let flat_nonces_table = txn.txn.open_table(&txn.tables.flat_nonces)?;
        let flat_deployed_contracts_table =
            txn.txn.open_table(&txn.tables.flat_deployed_contracts)?;
        let flat_compiled_class_hash_table =
            txn.txn.open_table(&txn.tables.flat_compiled_class_hash)?;
        Ok(StateReader {
            txn: &txn.txn,
            compiled_class_hash_table,
            declared_classes_table,
            declared_classes_block_table,
            deprecated_declared_classes_table,
            deprecated_declared_classes_block_table,
            deployed_contracts_table,
            nonces_table,
            storage_table,
            markers_table,
            file_handlers: &txn.file_handlers,
            flat_state: txn.flat_state,
            flat_contract_storage_table,
            flat_nonces_table,
            flat_deployed_contracts_table,
            flat_compiled_class_hash_table,
        })
    }

    fn get_state_marker(&self) -> StorageResult<BlockNumber> {
        Ok(self.markers_table.get(self.txn, &MarkerKind::State)?.unwrap_or_default())
    }

    /// Returns the class hash at a given state number.
    /// If class hash is not found, returns `None`.
    ///
    /// # Arguments
    /// * state_number - state number to search before.
    /// * address - contract addrest to search for.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    pub fn get_class_hash_at(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
    ) -> StorageResult<Option<ClassHash>> {
        if self.flat_state {
            let state_marker = self.get_state_marker()?;
            let block_number = state_number.block_after();
            if block_number >= state_marker {
                return Ok(self.flat_deployed_contracts_table.get(self.txn, address)?);
            }
            return Err(StorageError::DBInconsistency {
                msg: format!(
                    "Historical state query on flat_state node: state_number={state_number:?}, \
                     state_marker={state_marker}"
                ),
            });
        }
        self.get_class_hash_at_versioned(state_number, address)
    }

    fn get_class_hash_at_versioned(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
    ) -> StorageResult<Option<ClassHash>> {
        let first_irrelevant_block: BlockNumber = state_number.block_after();
        let db_key = (*address, first_irrelevant_block);
        let mut cursor = self.deployed_contracts_table.cursor(self.txn)?;
        cursor.lower_bound(&db_key)?;
        let res = cursor.prev()?;

        match res {
            None => Ok(None),
            Some(((got_address, _), _)) if got_address != *address => Ok(None),
            Some((_, class_hash)) => Ok(Some(class_hash)),
        }
    }

    /// Returns the class hash for a given contract address and block number using direct key-value
    /// lookup.
    ///
    /// # Arguments
    /// * address - contract address to search for.
    /// * block_number - block number to search at.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    pub fn get_class_hash_by_key(
        &self,
        address: &ContractAddress,
        block_number: BlockNumber,
    ) -> StorageResult<Option<ClassHash>> {
        let db_key = (*address, block_number);
        let class_hash = self.deployed_contracts_table.get(self.txn, &db_key)?;
        Ok(class_hash)
    }

    /// Returns the nonce at a given state number.
    /// If there is no nonce at the given state number, returns `None`.
    ///
    /// # Arguments
    /// * state_number - state number to search before.
    /// * address - contract addrest to search for.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    pub fn get_nonce_at(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
    ) -> StorageResult<Option<Nonce>> {
        if self.flat_state {
            let state_marker = self.get_state_marker()?;
            let block_number = state_number.block_after();
            if block_number >= state_marker {
                return Ok(self.flat_nonces_table.get(self.txn, address)?);
            }
            return Err(StorageError::DBInconsistency {
                msg: format!(
                    "Historical state query on flat_state node: state_number={state_number:?}, \
                     state_marker={state_marker}"
                ),
            });
        }
        // State diff updates are indexed by the block_number at which they occurred.
        let block_number: BlockNumber = state_number.block_after();
        get_nonce_at(block_number, address, self.txn, &self.nonces_table)
    }

    /// Returns the nonce for a given contract address and block number using direct key-value
    /// lookup.
    ///
    /// # Arguments
    /// * address - contract address to search for.
    /// * block_number - block number to search at.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    pub fn get_nonce_by_key(
        &self,
        address: &ContractAddress,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Nonce>> {
        let db_key = (*address, block_number);
        let nonce = self.nonces_table.get(self.txn, &db_key)?;
        Ok(nonce)
    }

    /// Returns the compiled class hash at a given state number.
    /// If CompiledClassHash is not found at the given state number, returns `None`.
    ///
    /// # Arguments
    /// * state_number - state number to search before.
    /// * class_hash - The class hash to search for.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    pub fn get_compiled_class_hash_at(
        &self,
        state_number: StateNumber,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<CompiledClassHash>> {
        if self.flat_state {
            let state_marker = self.get_state_marker()?;
            let block_number = state_number.block_after();
            if block_number >= state_marker {
                return Ok(self.flat_compiled_class_hash_table.get(self.txn, class_hash)?);
            }
            return Err(StorageError::DBInconsistency {
                msg: format!(
                    "Historical state query on flat_state node: state_number={state_number:?}, \
                     state_marker={state_marker}"
                ),
            });
        }
        // State diff updates are indexed by the block_number at which they occurred.
        let block_number: BlockNumber = state_number.block_after();
        get_compiled_class_hash_at(
            block_number,
            class_hash,
            self.txn,
            &self.compiled_class_hash_table,
        )
    }

    /// Returns the storage value at a given state number for a given contract and key.
    /// If no value is stored at the given state number, returns [`Felt`]::default.
    ///
    /// # Arguments
    /// * state_number - state number to search before.
    /// * address - contract addrest to search for.
    /// * key - key to search for.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    pub fn get_storage_at(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
        key: &StorageKey,
    ) -> StorageResult<Felt> {
        if self.flat_state {
            let state_marker = self.get_state_marker()?;
            let block_number = state_number.block_after();
            if block_number >= state_marker {
                return Ok(self
                    .flat_contract_storage_table
                    .get(self.txn, &(*address, *key))?
                    .unwrap_or_default());
            }
            return Err(StorageError::DBInconsistency {
                msg: format!(
                    "Historical state query on flat_state node: state_number={state_number:?}, \
                     state_marker={state_marker}"
                ),
            });
        }
        self.get_storage_at_versioned(state_number, address, key)
    }

    fn get_storage_at_versioned(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
        key: &StorageKey,
    ) -> StorageResult<Felt> {
        // The updates to the storage key are indexed by the block_number at which they occurred.
        let first_irrelevant_block: BlockNumber = state_number.block_after();
        // The relevant update is the last update strictly before `first_irrelevant_block`.
        let db_key = ((*address, *key), first_irrelevant_block);
        // Find the previous db item.
        let mut cursor = self.storage_table.cursor(self.txn)?;
        cursor.lower_bound(&db_key)?;
        let res = cursor.prev()?;
        match res {
            None => Ok(Felt::default()),
            Some((((got_address, got_key), _got_block_number), value)) => {
                if got_address != *address || got_key != *key {
                    // The previous item belongs to different key, which means there is no
                    // previous state diff for this item.
                    return Ok(Felt::default());
                };
                // The previous db item indeed belongs to this address and key.
                Ok(value)
            }
        }
    }

    /// Returns the storage value for a given contract address, storage key, and block number using
    /// direct key-value lookup.
    ///
    /// # Arguments
    /// * address - contract address to search for.
    /// * key - storage key to search for.
    /// * block_number - block number to search at.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    pub fn get_storage_by_key(
        &self,
        address: ContractAddress,
        key: StorageKey,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Felt>> {
        let db_key = ((address, key), block_number);
        let value = self.storage_table.get(self.txn, &db_key)?;
        Ok(value)
    }

    /// Returns the class definition at a given state number.
    ///
    /// If class_hash is not found, returns `None`.
    /// If class_hash is found but given state is before the block it's defined at, returns `None`.
    ///
    /// # Arguments
    /// * state_number - state number to search before.
    /// * class_hash - class hash to search for.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    ///
    /// Returns [`StorageError`]::DBInconsistency if the block number found for the class hash but
    /// the contract class was not found.
    pub fn get_class_definition_at(
        &self,
        state_number: StateNumber,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<SierraContractClass>> {
        let Some(block_number) = self.declared_classes_block_table.get(self.txn, class_hash)?
        else {
            return Ok(None);
        };
        if state_number.is_before(block_number) {
            return Ok(None);
        }
        // TODO(shahak): Fix code duplication with ClassStorageReader.
        let Some(contract_class_location) =
            self.declared_classes_table.get(self.txn, class_hash)?
        else {
            if state_number
                .is_after(self.markers_table.get(self.txn, &MarkerKind::Class)?.unwrap_or_default())
            {
                return Ok(None);
            }
            return Err(StorageError::DBInconsistency {
                msg: "Couldn't find class for a block that is before the class marker.".to_string(),
            });
        };
        Ok(Some(self.file_handlers.get_contract_class_unchecked(contract_class_location)?))
    }

    /// Returns the block number for a given class hash (the block in which it was defined).
    /// If class is not defined, returns `None`.
    ///
    /// # Arguments
    /// * class_hash - class hash to search for.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    pub fn get_class_definition_block_number(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<BlockNumber>> {
        Ok(self.declared_classes_block_table.get(self.txn, class_hash)?)
    }

    /// Returns the lowest block number for a given deprecated class hash (the first block in which
    /// it was defined). If the deprecated class is not defined, returns `None`.
    ///
    /// # Arguments
    /// * class_hash - class hash to search for.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    pub fn get_deprecated_class_definition_block_number(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<BlockNumber>> {
        Ok(self.deprecated_declared_classes_block_table.get(self.txn, class_hash)?)
    }

    /// Returns the deprecated contract class at a given state number for a given class hash.
    /// If class is not found, returns `None`.
    /// If class is defined but in a block after given state number, returns `None`.
    ///
    /// # Arguments
    /// * state_number - state number to search before.
    /// * class_hash - class hash to search for.
    ///
    /// # Errors
    /// Returns [`StorageError`] if there was an error searching the table.
    pub fn get_deprecated_class_definition_at(
        &self,
        state_number: StateNumber,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<DeprecatedContractClass>> {
        let Some(value) = self.deprecated_declared_classes_table.get(self.txn, class_hash)? else {
            return Ok(None);
        };
        if state_number.is_before(value.block_number) {
            return Ok(None);
        }
        // TODO(shahak): Fix code duplication with ClassStorageReader.
        Ok(Some(
            self.file_handlers.get_deprecated_contract_class_unchecked(value.location_in_file)?,
        ))
    }
}

impl StateStorageWriter for StorageTxn<'_, RW> {
    #[sequencer_latency_histogram(STORAGE_APPEND_THIN_STATE_DIFF_LATENCY, false)]
    fn append_state_diff(
        self,
        block_number: BlockNumber,
        thin_state_diff: ThinStateDiff,
    ) -> StorageResult<Self> {
        let file_offset_table = self.txn.open_table(&self.tables.file_offsets)?;
        let markers_table = self.open_table(&self.tables.markers)?;
        let state_diffs_table = self.open_table(&self.tables.state_diffs)?;
        let nonces_table = self.open_table(&self.tables.nonces)?;
        let deployed_contracts_table = self.open_table(&self.tables.deployed_contracts)?;
        let storage_table = self.open_table(&self.tables.contract_storage)?;
        let declared_classes_block_table = self.open_table(&self.tables.declared_classes_block)?;
        let deprecated_declared_classes_block_table =
            self.open_table(&self.tables.deprecated_declared_classes_block)?;
        let compiled_class_hash_table = self.open_table(&self.tables.compiled_class_hash)?;

        let flat_deployed_table = self.open_table(&self.tables.flat_deployed_contracts)?;
        let flat_nonces_table = self.open_table(&self.tables.flat_nonces)?;
        let flat_storage_table = self.open_table(&self.tables.flat_contract_storage)?;
        let flat_compiled_table = self.open_table(&self.tables.flat_compiled_class_hash)?;

        let changeset_deployed_table =
            self.open_table(&self.tables.changeset_deployed_contracts)?;
        let changeset_nonces_table = self.open_table(&self.tables.changeset_nonces)?;
        let changeset_storage_table = self.open_table(&self.tables.changeset_contract_storage)?;
        let changeset_compiled_table =
            self.open_table(&self.tables.changeset_compiled_class_hash)?;

        // Write state.
        write_deployed_contracts(
            &thin_state_diff.deployed_contracts,
            &self.txn,
            block_number,
            &deployed_contracts_table,
            &nonces_table,
            self.flat_state,
            &flat_deployed_table,
            &flat_nonces_table,
            &changeset_deployed_table,
            &changeset_nonces_table,
        )?;
        write_storage_diffs(
            &thin_state_diff.storage_diffs,
            &self.txn,
            block_number,
            &storage_table,
            self.flat_state,
            &flat_storage_table,
            &changeset_storage_table,
        )?;
        // Must be called after write_deployed_contracts since the nonces are updated there.
        write_nonces(
            &thin_state_diff.nonces,
            &self.txn,
            block_number,
            &nonces_table,
            self.flat_state,
            &flat_nonces_table,
            &changeset_nonces_table,
        )?;

        for (class_hash, _) in &thin_state_diff.class_hash_to_compiled_class_hash {
            let not_declared = declared_classes_block_table.get(&self.txn, class_hash)?.is_none();
            if not_declared {
                declared_classes_block_table.insert(&self.txn, class_hash, &block_number)?;
            }
        }

        write_compiled_class_hashes(
            &thin_state_diff.class_hash_to_compiled_class_hash,
            &self.txn,
            block_number,
            &compiled_class_hash_table,
            self.flat_state,
            &flat_compiled_table,
            &changeset_compiled_table,
        )?;

        for class_hash in thin_state_diff.deprecated_declared_classes.iter() {
            // Cairo0 classes can be declared in different blocks. The first block to declare the
            // class is recorded here.
            if deprecated_declared_classes_block_table.get(&self.txn, class_hash)?.is_none() {
                deprecated_declared_classes_block_table.insert(
                    &self.txn,
                    class_hash,
                    &block_number,
                )?;
            }
        }

        // Write state diff.
        let location = self.file_handlers.append_state_diff(&thin_state_diff);
        state_diffs_table.append(&self.txn, &block_number, &location)?;
        file_offset_table.upsert(&self.txn, &OffsetKind::ThinStateDiff, &location.next_offset())?;

        // Mark flat state as enabled (for toggle detection on restart).
        if self.flat_state && markers_table.get(&self.txn, &MarkerKind::FlatState)?.is_none() {
            markers_table.upsert(&self.txn, &MarkerKind::FlatState, &BlockNumber(1))?;
        }

        if self.flat_state {
            update_marker_to_next_block(
                &self.txn,
                &markers_table,
                MarkerKind::Changeset,
                block_number,
            )?;
            BATCHER_CHANGESET_MARKER.set_lossy(block_number.unchecked_next().0);
        }

        update_marker_to_next_block(&self.txn, &markers_table, MarkerKind::State, block_number)?;

        advance_compiled_class_marker_over_blocks_without_classes(
            &self.txn,
            &markers_table,
            &state_diffs_table,
            &self.file_handlers,
        )?;

        if let Some(retention_blocks) = self.changeset_retention_blocks {
            if self.flat_state {
                let changeset_tables = ChangesetTables {
                    deployed: changeset_deployed_table,
                    nonces: changeset_nonces_table,
                    storage: changeset_storage_table,
                    compiled: changeset_compiled_table,
                };
                prune_changesets(
                    &self.txn,
                    block_number,
                    retention_blocks,
                    &markers_table,
                    &changeset_tables,
                )?;
            }
        }

        Ok(self)
    }

    #[latency_histogram("storage_revert_state_diff_latency_seconds", false)]
    fn revert_state_diff(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<RevertedStateDiff>)> {
        let markers_table = self.open_table(&self.tables.markers)?;
        let declared_classes_table = self.open_table(&self.tables.declared_classes)?;
        let declared_classes_block_table = self.open_table(&self.tables.declared_classes_block)?;
        let deprecated_declared_classes_table =
            self.open_table(&self.tables.deprecated_declared_classes)?;
        let deprecated_declared_classes_block_table =
            self.open_table(&self.tables.deprecated_declared_classes_block)?;
        // TODO(yair): Consider reverting the compiled classes in their own module.
        let compiled_classes_table = self.open_table(&self.tables.casms)?;
        let compiled_class_hash_v2_table =
            self.open_table(&self.tables.stateless_compiled_class_hash_v2)?;
        let deployed_contracts_table = self.open_table(&self.tables.deployed_contracts)?;
        let nonces_table = self.open_table(&self.tables.nonces)?;
        let storage_table = self.open_table(&self.tables.contract_storage)?;
        let state_diffs_table = self.open_table(&self.tables.state_diffs)?;
        let compiled_class_hash_table = self.open_table(&self.tables.compiled_class_hash)?;

        let current_state_marker = self.get_state_marker()?;

        // Reverts only the last state diff.
        let Some(next_block_number) = block_number
            .next()
            .filter(|next_block_number| *next_block_number == current_state_marker)
        else {
            debug!(
                "Attempt to revert a non-existing / old state diff of block {}. Returning without \
                 an action.",
                block_number
            );
            return Ok((self, None));
        };

        let thin_state_diff = self
            .get_state_diff(block_number)?
            .unwrap_or_else(|| panic!("Missing state diff for block {block_number}."));
        markers_table.upsert(&self.txn, &MarkerKind::State, &block_number)?;
        let classes_marker = markers_table.get(&self.txn, &MarkerKind::Class)?.unwrap_or_default();
        if classes_marker == next_block_number {
            markers_table.upsert(&self.txn, &MarkerKind::Class, &block_number)?;
        }
        let compiled_classes_marker =
            markers_table.get(&self.txn, &MarkerKind::CompiledClass)?.unwrap_or_default();
        if compiled_classes_marker == next_block_number {
            markers_table.upsert(&self.txn, &MarkerKind::CompiledClass, &block_number)?;
        }
        let deleted_class_hashes = delete_declared_classes_block(
            &self.txn,
            &thin_state_diff,
            &declared_classes_block_table,
            block_number,
        )?;
        let deleted_classes = delete_declared_classes(
            &self.txn,
            &thin_state_diff,
            &declared_classes_table,
            &self.file_handlers,
        )?;
        let deleted_deprecated_class_hashes = delete_deprecated_declared_classes_block(
            &self.txn,
            block_number,
            &thin_state_diff,
            &deprecated_declared_classes_block_table,
        )?;
        let deleted_deprecated_classes = delete_deprecated_declared_classes(
            &self.txn,
            block_number,
            &thin_state_diff,
            &deprecated_declared_classes_table,
            &self.file_handlers,
        )?;
        let deleted_compiled_classes = delete_compiled_classes(
            &self.txn,
            thin_state_diff.class_hash_to_compiled_class_hash.keys(),
            &compiled_classes_table,
            &self.file_handlers,
        )?;
        delete_compiled_class_hashes_v2(
            &self.txn,
            thin_state_diff.class_hash_to_compiled_class_hash.keys(),
            &compiled_class_hash_v2_table,
        )?;
        // Restore flat tables from changeset tables, with assertions comparing against versioned
        // table lookups.
        if self.flat_state {
            let changeset_pruned_marker =
                markers_table.get(&self.txn, &MarkerKind::ChangesetPruned)?.unwrap_or_default();
            if block_number < changeset_pruned_marker {
                return Err(StorageError::RevertBeyondChangesetHistory {
                    block_number,
                    oldest_changeset: changeset_pruned_marker,
                });
            }

            let flat_deployed_table = self.open_table(&self.tables.flat_deployed_contracts)?;
            let flat_nonces_table = self.open_table(&self.tables.flat_nonces)?;
            let flat_storage_table = self.open_table(&self.tables.flat_contract_storage)?;
            let flat_compiled_table = self.open_table(&self.tables.flat_compiled_class_hash)?;

            let changeset_deployed_table =
                self.open_table(&self.tables.changeset_deployed_contracts)?;
            let changeset_nonces_table = self.open_table(&self.tables.changeset_nonces)?;
            let changeset_storage_table =
                self.open_table(&self.tables.changeset_contract_storage)?;
            let changeset_compiled_table =
                self.open_table(&self.tables.changeset_compiled_class_hash)?;

            // Restore deployed_contracts from changeset.
            for (address, _) in &thin_state_diff.deployed_contracts {
                let changeset_preimage =
                    changeset_deployed_table.get(&self.txn, &(block_number, *address))?.flatten();

                match changeset_preimage {
                    Some(hash) => flat_deployed_table.upsert(&self.txn, address, &hash)?,
                    None => {
                        let _ = flat_deployed_table.delete(&self.txn, address);
                    }
                }
            }

            // Restore nonces from changeset using cursor iteration (covers both nonces diff and
            // deployed contracts with implicit nonce initialization).
            {
                let mut cursor = changeset_nonces_table.cursor(&self.txn)?;
                let mut entry = cursor.lower_bound(&(block_number, ContractAddress::default()))?;
                while let Some(((got_block, address), preimage)) = entry {
                    if got_block != block_number {
                        break;
                    }

                    match preimage {
                        Some(nonce) => flat_nonces_table.upsert(&self.txn, &address, &nonce)?,
                        None => {
                            let _ = flat_nonces_table.delete(&self.txn, &address);
                        }
                    }

                    entry = cursor.next()?;
                }
            }

            // Restore storage from changeset.
            for (address, diffs) in &thin_state_diff.storage_diffs {
                for (key, _) in diffs {
                    let changeset_preimage = changeset_storage_table
                        .get(&self.txn, &((block_number, *address), *key))?
                        .flatten();

                    match changeset_preimage {
                        Some(value) if value != Felt::default() => {
                            flat_storage_table.upsert(&self.txn, &(*address, *key), &value)?;
                        }
                        _ => {
                            let _ = flat_storage_table.delete(&self.txn, &(*address, *key));
                        }
                    }
                }
            }

            // Restore compiled_class_hash from changeset.
            for (hash, _) in &thin_state_diff.class_hash_to_compiled_class_hash {
                let changeset_preimage =
                    changeset_compiled_table.get(&self.txn, &(block_number, *hash))?.flatten();

                match changeset_preimage {
                    Some(compiled) => flat_compiled_table.upsert(&self.txn, hash, &compiled)?,
                    None => {
                        let _ = flat_compiled_table.delete(&self.txn, hash);
                    }
                }
            }

            // Delete changeset entries for the reverted block.
            for (address, _) in &thin_state_diff.deployed_contracts {
                changeset_deployed_table.delete(&self.txn, &(block_number, *address))?;
            }
            {
                let mut nonce_addresses_to_delete = Vec::new();
                let mut cursor = changeset_nonces_table.cursor(&self.txn)?;
                let mut entry = cursor.lower_bound(&(block_number, ContractAddress::default()))?;
                while let Some(((got_block, address), _)) = entry {
                    if got_block != block_number {
                        break;
                    }
                    nonce_addresses_to_delete.push(address);
                    entry = cursor.next()?;
                }
                for address in nonce_addresses_to_delete {
                    changeset_nonces_table.delete(&self.txn, &(block_number, address))?;
                }
            }
            for (address, diffs) in &thin_state_diff.storage_diffs {
                for (key, _) in diffs {
                    changeset_storage_table.delete(&self.txn, &((block_number, *address), *key))?;
                }
            }
            for (hash, _) in &thin_state_diff.class_hash_to_compiled_class_hash {
                changeset_compiled_table.delete(&self.txn, &(block_number, *hash))?;
            }

            // Decrement changeset marker.
            markers_table.upsert(&self.txn, &MarkerKind::Changeset, &block_number)?;
            BATCHER_CHANGESET_MARKER.set_lossy(block_number.0);
        }

        if !self.flat_state {
            delete_deployed_contracts(
                &self.txn,
                block_number,
                &thin_state_diff,
                &deployed_contracts_table,
                &nonces_table,
            )?;
            delete_storage_diffs(&self.txn, block_number, &thin_state_diff, &storage_table)?;
            delete_nonces(&self.txn, block_number, &thin_state_diff, &nonces_table)?;
            delete_compiled_class_hashes(
                &self.txn,
                block_number,
                &thin_state_diff,
                &compiled_class_hash_table,
            )?;
        }
        state_diffs_table.delete(&self.txn, &block_number)?;

        Ok((
            self,
            Some((
                thin_state_diff,
                deleted_class_hashes,
                deleted_classes,
                deleted_deprecated_class_hashes,
                deleted_deprecated_classes,
                deleted_compiled_classes,
            )),
        ))
    }
}

#[latency_histogram("storage_update_marker_to_next_block_latency_seconds", true)]
fn update_marker_to_next_block<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    marker_kind: MarkerKind,
    block_number: BlockNumber,
) -> StorageResult<()> {
    // Make sure marker is consistent.
    let marker = markers_table.get(txn, &marker_kind)?.unwrap_or_default();
    if marker != block_number {
        return Err(StorageError::MarkerMismatch {
            marker_kind,
            expected: marker,
            found: block_number,
        });
    };

    // Advance marker.
    markers_table.upsert(txn, &marker_kind, &block_number.unchecked_next())?;
    Ok(())
}

/// Maximum number of blocks to prune in a single `append_state_diff` call.
/// Bounds overhead on the critical write path. Large backlogs converge gradually.
const MAX_BLOCKS_TO_PRUNE_PER_APPEND: u64 = 100;

// TODO(phase3-optimization): If inline pruning overhead becomes measurable on the critical
// write path, move to a background task with batched deletes in separate transactions.

// TODO(phase3-l1-finality): Replace fixed block count with L1 finality-based pruning target.
// The pruning mechanics stay the same — only the source of prune_target changes.
// The L1 finality point would be passed down from the batcher (which has L1 provider access)
// as a parameter to append_state_diff, or set as a "prune target" on the storage transaction.

#[allow(clippy::too_many_arguments)]
fn prune_changesets<'env>(
    txn: &DbTransaction<'env, RW>,
    current_block: BlockNumber,
    retention_blocks: u64,
    markers_table: &MarkersTable<'env>,
    changeset_tables: &ChangesetTables<'env>,
) -> StorageResult<()> {
    let prune_target = BlockNumber(current_block.0.saturating_sub(retention_blocks));
    let changeset_pruned_marker =
        markers_table.get(txn, &MarkerKind::ChangesetPruned)?.unwrap_or_default();

    if changeset_pruned_marker >= prune_target {
        return Ok(());
    }

    let prune_end = BlockNumber(std::cmp::min(
        prune_target.0,
        changeset_pruned_marker.0.saturating_add(MAX_BLOCKS_TO_PRUNE_PER_APPEND),
    ));

    for block_num in changeset_pruned_marker.0..prune_end.0 {
        delete_changeset_entries_for_block(txn, BlockNumber(block_num), changeset_tables)?;
    }

    markers_table.upsert(txn, &MarkerKind::ChangesetPruned, &prune_end)?;
    BATCHER_CHANGESET_PRUNED_MARKER.set_lossy(prune_end.0);
    Ok(())
}

/// Deletes all changeset entries for the given block number from all 4 changeset tables.
fn delete_changeset_entries_for_block<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    tables: &ChangesetTables<'env>,
) -> StorageResult<()> {
    // Delete deployed contracts changeset entries.
    {
        let mut keys_to_delete = Vec::new();
        let mut cursor = tables.deployed.cursor(txn)?;
        let mut entry = cursor.lower_bound(&(block_number, ContractAddress::default()))?;
        while let Some(((got_block, address), _)) = entry {
            if got_block != block_number {
                break;
            }
            keys_to_delete.push(address);
            entry = cursor.next()?;
        }
        for address in keys_to_delete {
            tables.deployed.delete(txn, &(block_number, address))?;
        }
    }

    // Delete nonces changeset entries.
    {
        let mut keys_to_delete = Vec::new();
        let mut cursor = tables.nonces.cursor(txn)?;
        let mut entry = cursor.lower_bound(&(block_number, ContractAddress::default()))?;
        while let Some(((got_block, address), _)) = entry {
            if got_block != block_number {
                break;
            }
            keys_to_delete.push(address);
            entry = cursor.next()?;
        }
        for address in keys_to_delete {
            tables.nonces.delete(txn, &(block_number, address))?;
        }
    }

    // Delete contract storage changeset entries.
    {
        let mut keys_to_delete = Vec::new();
        let mut cursor = tables.storage.cursor(txn)?;
        let mut entry = cursor
            .lower_bound(&((block_number, ContractAddress::default()), StorageKey::default()))?;
        while let Some((((got_block, address), key), _)) = entry {
            if got_block != block_number {
                break;
            }
            keys_to_delete.push((address, key));
            entry = cursor.next()?;
        }
        for (address, key) in keys_to_delete {
            tables.storage.delete(txn, &((block_number, address), key))?;
        }
    }

    // Delete compiled class hash changeset entries.
    {
        let mut keys_to_delete = Vec::new();
        let mut cursor = tables.compiled.cursor(txn)?;
        let mut entry = cursor.lower_bound(&(block_number, ClassHash::default()))?;
        while let Some(((got_block, hash), _)) = entry {
            if got_block != block_number {
                break;
            }
            keys_to_delete.push(hash);
            entry = cursor.next()?;
        }
        for hash in keys_to_delete {
            tables.compiled.delete(txn, &(block_number, hash))?;
        }
    }

    Ok(())
}

#[latency_histogram(
    "storage_advance_compiled_class_marker_over_blocks_without_classes_latency_seconds",
    true
)]
fn advance_compiled_class_marker_over_blocks_without_classes<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    state_diffs_table: &'env TableHandle<
        '_,
        BlockNumber,
        VersionZeroWrapper<LocationInFile>,
        SimpleTable,
    >,
    file_handlers: &FileHandlers<RW>,
) -> StorageResult<()> {
    let state_marker = markers_table.get(txn, &MarkerKind::State)?.unwrap_or_default();
    let mut compiled_class_marker =
        markers_table.get(txn, &MarkerKind::CompiledClass)?.unwrap_or_default();
    while compiled_class_marker < state_marker {
        let state_diff_location = state_diffs_table
            .get(txn, &compiled_class_marker)?
            .unwrap_or_else(|| panic!("Missing state diff for block {compiled_class_marker}"));
        if !file_handlers
            .get_thin_state_diff_unchecked(state_diff_location)?
            .class_hash_to_compiled_class_hash
            .is_empty()
        {
            break;
        }
        compiled_class_marker = compiled_class_marker.unchecked_next();
        markers_table.upsert(txn, &MarkerKind::CompiledClass, &compiled_class_marker)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[latency_histogram("storage_write_deployed_contracts_latency_seconds", true)]
fn write_deployed_contracts<'env>(
    deployed_contracts: &IndexMap<ContractAddress, ClassHash>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    deployed_contracts_table: &'env DeployedContractsTable<'env>,
    nonces_table: &'env NoncesTable<'env>,
    flat_state: bool,
    flat_deployed_table: &'env FlatDeployedContractsTable<'env>,
    flat_nonces_table: &'env FlatNoncesTable<'env>,
    changeset_deployed_table: &'env ChangesetDeployedContractsTable<'env>,
    changeset_nonces_table: &'env ChangesetNoncesTable<'env>,
) -> StorageResult<()> {
    for (address, class_hash) in deployed_contracts {
        if flat_state {
            // Capture deployed contract pre-image from flat table.
            let prior_class_hash = flat_deployed_table.get(txn, address)?;
            changeset_deployed_table.upsert(txn, &(block_number, *address), &prior_class_hash)?;

            // Capture nonce pre-image from flat table BEFORE implicit nonce initialization.
            let prior_nonce = flat_nonces_table.get(txn, address)?;
            changeset_nonces_table.upsert(txn, &(block_number, *address), &prior_nonce)?;
        }

        if !flat_state {
            deployed_contracts_table.insert(txn, &(*address, block_number), class_hash)?;
        }
        if flat_state {
            flat_deployed_table.upsert(txn, address, class_hash)?;
        }

        // In old blocks, there is no nonce diff, so we must add the default value for newly
        // deployed contracts. Replaced classes will already have a nonce and thus won't enter this
        // condition.
        if !flat_state && get_nonce_at(block_number, address, txn, nonces_table)?.is_none() {
            nonces_table.append_greater_sub_key(
                txn,
                &(*address, block_number),
                &Nonce::default(),
            )?;
        }
        if flat_state && flat_nonces_table.get(txn, address)?.is_none() {
            flat_nonces_table.upsert(txn, address, &Nonce::default())?;
        }
    }
    Ok(())
}

#[latency_histogram("storage_write_nonce_latency_seconds", false)]
fn write_nonces<'env>(
    nonces: &IndexMap<ContractAddress, Nonce>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    contracts_table: &'env NoncesTable<'env>,
    flat_state: bool,
    flat_nonces_table: &'env FlatNoncesTable<'env>,
    changeset_nonces_table: &'env ChangesetNoncesTable<'env>,
) -> StorageResult<()> {
    for (contract_address, nonce) in nonces {
        if flat_state {
            // Only write changeset if not already captured by write_deployed_contracts.
            if changeset_nonces_table.get(txn, &(block_number, *contract_address))?.is_none() {
                let prior_nonce = flat_nonces_table.get(txn, contract_address)?;
                changeset_nonces_table.upsert(
                    txn,
                    &(block_number, *contract_address),
                    &prior_nonce,
                )?;
            }
        }
        if !flat_state {
            contracts_table.upsert(txn, &(*contract_address, block_number), nonce)?;
        }
        if flat_state {
            flat_nonces_table.upsert(txn, contract_address, nonce)?;
        }
    }
    Ok(())
}

#[latency_histogram("storage_write_nonce_latency_seconds", false)]
fn write_compiled_class_hashes<'env>(
    compiled_class_hashes: &IndexMap<ClassHash, CompiledClassHash>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    compiled_class_hash_table: &'env CompiledClassHashTable<'env>,
    flat_state: bool,
    flat_compiled_table: &'env FlatCompiledClassHashTable<'env>,
    changeset_compiled_table: &'env ChangesetCompiledClassHashTable<'env>,
) -> StorageResult<()> {
    for (class_hash, compiled_class_hash) in compiled_class_hashes {
        if flat_state {
            let prior_compiled = flat_compiled_table.get(txn, class_hash)?;
            changeset_compiled_table.upsert(txn, &(block_number, *class_hash), &prior_compiled)?;
        }
        if !flat_state {
            compiled_class_hash_table.insert(
                txn,
                &(*class_hash, block_number),
                compiled_class_hash,
            )?;
        }
        if flat_state {
            flat_compiled_table.upsert(txn, class_hash, compiled_class_hash)?;
        }
    }
    Ok(())
}

#[latency_histogram("storage_write_storage_diffs_latency_seconds", false)]
fn write_storage_diffs<'env>(
    storage_diffs: &IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    storage_table: &'env ContractStorageTable<'env>,
    flat_state: bool,
    flat_storage_table: &'env FlatContractStorageTable<'env>,
    changeset_storage_table: &'env ChangesetContractStorageTable<'env>,
) -> StorageResult<()> {
    for (address, storage_entries) in storage_diffs {
        for (key, value) in storage_entries {
            if flat_state {
                let prior_value = flat_storage_table.get(txn, &(*address, *key))?;
                changeset_storage_table.upsert(
                    txn,
                    &((block_number, *address), *key),
                    &prior_value,
                )?;
            }
            if !flat_state {
                storage_table.append_greater_sub_key(
                    txn,
                    &((*address, *key), block_number),
                    value,
                )?;
            }
            if flat_state {
                flat_storage_table.upsert(txn, &(*address, *key), value)?;
            }
        }
    }
    Ok(())
}

fn delete_declared_classes_block<'env>(
    txn: &'env DbTransaction<'env, RW>,
    thin_state_diff: &ThinStateDiff,
    declared_classes_block_table: &'env DeclaredClassesBlockTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<Vec<ClassHash>> {
    let mut deleted_data = Vec::new();
    for class_hash in thin_state_diff.class_hash_to_compiled_class_hash.keys() {
        let class_block_entry =
            declared_classes_block_table.get(txn, class_hash)?.ok_or_else(|| {
                StorageError::DBInconsistency {
                    msg: format!(
                        "Attempting to revert declaration of class {class_hash} but it doesn't \
                         exist in the DB"
                    ),
                }
            })?;
        if class_block_entry == block_number {
            declared_classes_block_table.delete(txn, class_hash)?;
            deleted_data.push(*class_hash);
        }
    }
    Ok(deleted_data)
}

fn delete_declared_classes<'env>(
    txn: &'env DbTransaction<'env, RW>,
    thin_state_diff: &ThinStateDiff,
    declared_classes_table: &'env DeclaredClassesTable<'env>,
    file_handlers: &FileHandlers<RW>,
) -> StorageResult<IndexMap<ClassHash, SierraContractClass>> {
    let mut deleted_data = IndexMap::new();
    for class_hash in thin_state_diff.class_hash_to_compiled_class_hash.keys() {
        let Some(contract_class_location) = declared_classes_table.get(txn, class_hash)? else {
            continue;
        };
        deleted_data.insert(
            *class_hash,
            file_handlers.get_contract_class_unchecked(contract_class_location)?,
        );
        declared_classes_table.delete(txn, class_hash)?;
    }

    Ok(deleted_data)
}
fn delete_deprecated_declared_classes_block<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    deprecated_declared_classes_block_table: &'env DeprecatedDeclaredClassesBlockTable<'env>,
) -> StorageResult<Vec<ClassHash>> {
    let mut deleted_data = Vec::new();
    for class_hash in thin_state_diff.deprecated_declared_classes.iter() {
        let declared_block_number = deprecated_declared_classes_block_table
            .get(txn, class_hash)?
            .ok_or_else(|| StorageError::DBInconsistency {
            msg: format!(
                "Attempting to revert declaration of class {class_hash} but it doesn't exist in \
                 the DB"
            ),
        })?;

        if block_number < declared_block_number {
            return Err(StorageError::DBInconsistency {
                msg: format!(
                    "Attempting to revert class {class_hash} at block {block_number} but DB shows \
                     it was first declared at later block {declared_block_number}"
                ),
            });
        }

        // Delete the class from the table only if it was first declared in this block.
        if block_number == declared_block_number {
            deprecated_declared_classes_block_table.delete(txn, class_hash)?;
            deleted_data.push(*class_hash);
        }
    }
    Ok(deleted_data)
}

fn delete_deprecated_declared_classes<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    deprecated_declared_classes_table: &'env DeprecatedDeclaredClassesTable<'env>,
    file_handlers: &FileHandlers<RW>,
) -> StorageResult<IndexMap<ClassHash, DeprecatedContractClass>> {
    // Class hashes of the contracts that were deployed in this block.
    let deployed_contracts_class_hashes = thin_state_diff.deployed_contracts.values();

    // Merge the class hashes from the state diff and from the deployed contracts into a single
    // unique set.
    let class_hashes: HashSet<&ClassHash> = thin_state_diff
        .deprecated_declared_classes
        .iter()
        .chain(deployed_contracts_class_hashes)
        .collect();

    let mut deleted_data = IndexMap::new();
    for class_hash in class_hashes {
        // If the class is not in the deprecated classes table, it means that either we didn't
        // download it yet or the hash is of a deployed contract of a new class type. We've decided
        // to avoid deleting these classes because they're from at most 0.11.
        if let Some(IndexedDeprecatedContractClass {
            block_number: declared_block_number,
            location_in_file,
        }) = deprecated_declared_classes_table.get(txn, class_hash)?
        {
            // If the class was declared in a different block then we should'nt delete it.
            if block_number == declared_block_number {
                deleted_data.insert(
                    *class_hash,
                    file_handlers.get_deprecated_contract_class_unchecked(location_in_file)?,
                );
                deprecated_declared_classes_table.delete(txn, class_hash)?;
            }
        }
    }

    Ok(deleted_data)
}

fn delete_compiled_classes<'a, 'env>(
    txn: &'env DbTransaction<'env, RW>,
    class_hashes: impl Iterator<Item = &'a ClassHash>,
    compiled_classes_table: &'env CompiledClassesTable<'env>,
    file_handlers: &FileHandlers<RW>,
) -> StorageResult<IndexMap<ClassHash, CasmContractClass>> {
    let mut deleted_data = IndexMap::new();
    for class_hash in class_hashes {
        let Some(compiled_class_location) = compiled_classes_table.get(txn, class_hash)?
        // No compiled class means the rest of the compiled classes weren't downloaded yet.
        else {
            break;
        };
        compiled_classes_table.delete(txn, class_hash)?;
        deleted_data
            .insert(*class_hash, file_handlers.get_casm_unchecked(compiled_class_location)?);
    }

    Ok(deleted_data)
}

fn delete_deployed_contracts<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    deployed_contracts_table: &'env DeployedContractsTable<'env>,
    nonces_table: &'env NoncesTable<'env>,
) -> StorageResult<()> {
    for contract_address in thin_state_diff.deployed_contracts.keys() {
        deployed_contracts_table.delete(txn, &(*contract_address, block_number))?;
        // Delete the nonce if the contract was deployed in this block (i.e didn't have a nonce in
        // the previous block).
        if get_nonce_at(block_number, contract_address, txn, nonces_table)?.is_none() {
            nonces_table.delete(txn, &(*contract_address, block_number))?;
        }
    }
    Ok(())
}

#[latency_histogram("storage_delete_storage_diffs_latency_seconds", false)]
fn delete_storage_diffs<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    storage_table: &'env ContractStorageTable<'env>,
) -> StorageResult<()> {
    for (address, storage_entries) in &thin_state_diff.storage_diffs {
        for (key, _) in storage_entries {
            storage_table.delete(txn, &((*address, *key), block_number))?;
        }
    }
    Ok(())
}

#[latency_histogram("storage_delete_nonces_latency_seconds", false)]
fn delete_nonces<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    contracts_table: &'env NoncesTable<'env>,
) -> StorageResult<()> {
    for contract_address in thin_state_diff.nonces.keys() {
        contracts_table.delete(txn, &(*contract_address, block_number))?;
    }
    Ok(())
}

fn delete_compiled_class_hashes<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    compiled_class_hash_table: &'env CompiledClassHashTable<'env>,
) -> StorageResult<()> {
    for (class_hash, _) in &thin_state_diff.class_hash_to_compiled_class_hash {
        compiled_class_hash_table.delete(txn, &(*class_hash, block_number))?;
    }
    Ok(())
}

fn delete_compiled_class_hashes_v2<'env>(
    txn: &'env DbTransaction<'env, RW>,
    class_hashes: impl Iterator<Item = &'env ClassHash>,
    compiled_class_hash_v2_table: &'env StatelessCompiledClassHashV2Table<'env>,
) -> StorageResult<()> {
    for class_hash in class_hashes {
        compiled_class_hash_v2_table.delete(txn, class_hash)?;
    }
    Ok(())
}

fn get_nonce_at<'env, Mode: TransactionKind>(
    first_irrelevant_block: BlockNumber,
    address: &ContractAddress,
    txn: &'env DbTransaction<'env, Mode>,
    nonces_table: &'env NoncesTable<'env>,
) -> StorageResult<Option<Nonce>> {
    // The relevant update is the last update strictly before `first_irrelevant_block`.
    let db_key = (*address, first_irrelevant_block);
    // Find the previous db item.
    let mut cursor = nonces_table.cursor(txn)?;
    cursor.lower_bound(&db_key)?;
    let res = cursor.prev()?;
    match res {
        None => Ok(None),
        Some(((got_address, _got_block_number), value)) => {
            if got_address != *address {
                // The previous item belongs to different address, which means there is no
                // previous state diff for this item.
                return Ok(None);
            };
            // The previous db item indeed belongs to this address and key.
            Ok(Some(value))
        }
    }
}

fn get_compiled_class_hash_at<'env, Mode: TransactionKind>(
    first_irrelevant_block: BlockNumber,
    class_hash: &ClassHash,
    txn: &'env DbTransaction<'env, Mode>,
    compiled_class_hash_table: &'env CompiledClassHashTable<'env>,
) -> StorageResult<Option<CompiledClassHash>> {
    let db_key = (*class_hash, first_irrelevant_block);
    // Find the previous db item.
    let mut cursor = compiled_class_hash_table.cursor(txn)?;
    cursor.lower_bound(&db_key)?;
    let res = cursor.prev()?;
    match res {
        None => Ok(None),
        Some(((got_class_hash, _got_block_number), value)) => {
            if got_class_hash != *class_hash {
                // The previous item belongs to different class hash, which means there is no
                // previous state diff for this item.
                return Ok(None);
            };
            // The previous db item indeed belongs to this address and key.
            Ok(Some(value))
        }
    }
}
