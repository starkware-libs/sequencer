//! Flat state write and revert functions for sequencer mode.
//!
//! In sequencer mode, state is stored in flat tables (current value only) with pre-image tables
//! for revert support. This module contains the write functions that replace the versioned-table
//! writes used in archive mode.

use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_types_core::felt::Felt;

use super::presence_prefixed::PresencePrefixed;
use crate::db::serialization::{NoVersionValueWrapper, VersionZeroWrapper};
use crate::db::table_types::{CommonPrefix, SimpleTable, Table};
use crate::db::{DbTransaction, TableHandle, RW};
use crate::mmap_file::LocationInFile;
use crate::state::data::IndexedDeprecatedContractClass;
use crate::{BlockNumber, StorageResult};

// Type aliases for flat tables.
pub(crate) type FlatContractStorageTable<'env> =
    TableHandle<'env, (ContractAddress, StorageKey), NoVersionValueWrapper<Felt>, CommonPrefix>;
pub(crate) type FlatNoncesTable<'env> =
    TableHandle<'env, ContractAddress, NoVersionValueWrapper<Nonce>, SimpleTable>;
pub(crate) type FlatDeployedContractsTable<'env> =
    TableHandle<'env, ContractAddress, NoVersionValueWrapper<ClassHash>, SimpleTable>;
pub(crate) type FlatCompiledClassHashTable<'env> =
    TableHandle<'env, ClassHash, NoVersionValueWrapper<CompiledClassHash>, SimpleTable>;

// Type aliases for pre-image tables.
pub(crate) type StoragePreimagesTable<'env> = TableHandle<
    'env,
    ((BlockNumber, ContractAddress), StorageKey),
    NoVersionValueWrapper<PresencePrefixed<Felt>>,
    CommonPrefix,
>;
pub(crate) type NoncePreimagesTable<'env> = TableHandle<
    'env,
    (BlockNumber, ContractAddress),
    NoVersionValueWrapper<PresencePrefixed<Nonce>>,
    CommonPrefix,
>;
pub(crate) type DeployedContractPreimagesTable<'env> = TableHandle<
    'env,
    (BlockNumber, ContractAddress),
    NoVersionValueWrapper<PresencePrefixed<ClassHash>>,
    CommonPrefix,
>;
pub(crate) type ClassPreimagesTable<'env> = TableHandle<
    'env,
    (BlockNumber, ClassHash),
    NoVersionValueWrapper<CompiledClassHash>,
    CommonPrefix,
>;

// Existing table type aliases needed for class declarations.
pub(crate) type DeclaredClassesBlockTable<'env> =
    TableHandle<'env, ClassHash, NoVersionValueWrapper<BlockNumber>, SimpleTable>;
pub(crate) type DeprecatedDeclaredClassesBlockTable<'env> =
    TableHandle<'env, ClassHash, NoVersionValueWrapper<BlockNumber>, SimpleTable>;

// Class visibility table type aliases (for revert).
pub(crate) type DeclaredClassesTable<'env> =
    TableHandle<'env, ClassHash, VersionZeroWrapper<LocationInFile>, SimpleTable>;
pub(crate) type CompiledClassesTable<'env> =
    TableHandle<'env, ClassHash, VersionZeroWrapper<LocationInFile>, SimpleTable>;
pub(crate) type DeprecatedDeclaredClassesTable<'env> =
    TableHandle<'env, ClassHash, VersionZeroWrapper<IndexedDeprecatedContractClass>, SimpleTable>;

/// All flat state and pre-image table handles needed for write operations.
pub(crate) struct FlatStateTables<'env> {
    pub flat_contract_storage: FlatContractStorageTable<'env>,
    pub flat_nonces: FlatNoncesTable<'env>,
    pub flat_deployed_contracts: FlatDeployedContractsTable<'env>,
    pub flat_compiled_class_hash: FlatCompiledClassHashTable<'env>,
    pub storage_preimages: StoragePreimagesTable<'env>,
    pub nonce_preimages: NoncePreimagesTable<'env>,
    pub deployed_contract_preimages: DeployedContractPreimagesTable<'env>,
    pub class_preimages: ClassPreimagesTable<'env>,
    pub declared_classes_block: DeclaredClassesBlockTable<'env>,
    pub deprecated_declared_classes_block: DeprecatedDeclaredClassesBlockTable<'env>,
}

/// Additional table handles needed for revert (class visibility cleanup).
pub(crate) struct FlatStateRevertTables<'env> {
    pub base: FlatStateTables<'env>,
    pub declared_classes: DeclaredClassesTable<'env>,
    pub compiled_classes: CompiledClassesTable<'env>,
    pub deprecated_declared_classes: DeprecatedDeclaredClassesTable<'env>,
}

/// Write flat state + pre-images for a block's state diff.
pub(crate) fn write_flat_state_diff<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    tables: &'env FlatStateTables<'env>,
) -> StorageResult<()> {
    // 1. Deployed contracts + implicit nonce init (must come before explicit nonces).
    write_flat_deployed_contracts(
        txn,
        block_number,
        thin_state_diff,
        &tables.flat_deployed_contracts,
        &tables.deployed_contract_preimages,
        &tables.flat_nonces,
        &tables.nonce_preimages,
    )?;

    // 2. Storage diffs.
    write_flat_storage_diffs(
        txn,
        block_number,
        thin_state_diff,
        &tables.flat_contract_storage,
        &tables.storage_preimages,
    )?;

    // 3. Explicit nonces (must come after deployed contracts for coalescing).
    write_flat_nonces(
        txn,
        block_number,
        thin_state_diff,
        &tables.flat_nonces,
        &tables.nonce_preimages,
    )?;

    // 4. Class declarations.
    write_flat_compiled_class_hashes(
        txn,
        block_number,
        thin_state_diff,
        &tables.flat_compiled_class_hash,
        &tables.class_preimages,
        &tables.declared_classes_block,
    )?;

    // 5. Deprecated class declarations (same as archive — first declaration only).
    write_deprecated_declared_classes_block(
        txn,
        thin_state_diff,
        &tables.deprecated_declared_classes_block,
        block_number,
    )?;

    Ok(())
}

fn write_flat_storage_diffs<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    flat_table: &'env FlatContractStorageTable<'env>,
    preimages_table: &'env StoragePreimagesTable<'env>,
) -> StorageResult<()> {
    for (address, storage_diffs) in &thin_state_diff.storage_diffs {
        for (key, value) in storage_diffs {
            let current = flat_table.get(txn, &(*address, *key))?;
            preimages_table.insert(txn, &((block_number, *address), *key), &current.into())?;
            flat_table.upsert(txn, &(*address, *key), value)?;
        }
    }
    Ok(())
}

fn write_flat_deployed_contracts<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    flat_deployed_table: &'env FlatDeployedContractsTable<'env>,
    deployed_preimages_table: &'env DeployedContractPreimagesTable<'env>,
    flat_nonces_table: &'env FlatNoncesTable<'env>,
    nonce_preimages_table: &'env NoncePreimagesTable<'env>,
) -> StorageResult<()> {
    for (address, class_hash) in &thin_state_diff.deployed_contracts {
        // Pre-image for deployed_contracts.
        let current_class = flat_deployed_table.get(txn, address)?;
        deployed_preimages_table.insert(txn, &(block_number, *address), &current_class.into())?;
        flat_deployed_table.upsert(txn, address, class_hash)?;

        // Implicit nonce initialization for new deployments.
        if flat_nonces_table.get(txn, address)?.is_none() {
            nonce_preimages_table.insert(
                txn,
                &(block_number, *address),
                &PresencePrefixed::absent(),
            )?;
            flat_nonces_table.upsert(txn, address, &Nonce::default())?;
        }
    }
    Ok(())
}

fn write_flat_nonces<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    flat_nonces_table: &'env FlatNoncesTable<'env>,
    nonce_preimages_table: &'env NoncePreimagesTable<'env>,
) -> StorageResult<()> {
    for (address, nonce) in &thin_state_diff.nonces {
        // Check if a pre-image was already written by deploy in same block (coalescing).
        let already_has_preimage =
            nonce_preimages_table.get(txn, &(block_number, *address))?.is_some();
        if !already_has_preimage {
            let current = flat_nonces_table.get(txn, address)?;
            nonce_preimages_table.insert(txn, &(block_number, *address), &current.into())?;
        }
        // Always write the explicit nonce (may overwrite deploy default).
        flat_nonces_table.upsert(txn, address, nonce)?;
    }
    Ok(())
}

fn write_flat_compiled_class_hashes<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    flat_compiled_hash_table: &'env FlatCompiledClassHashTable<'env>,
    class_preimages_table: &'env ClassPreimagesTable<'env>,
    declared_classes_block_table: &'env DeclaredClassesBlockTable<'env>,
) -> StorageResult<()> {
    for (class_hash, compiled_class_hash) in &thin_state_diff.class_hash_to_compiled_class_hash {
        // Class preimage (for revert).
        class_preimages_table.insert(txn, &(block_number, *class_hash), compiled_class_hash)?;
        // Flat compiled class hash.
        flat_compiled_hash_table.upsert(txn, class_hash, compiled_class_hash)?;
        // Declared classes block (first declaration only, same as archive).
        if declared_classes_block_table.get(txn, class_hash)?.is_none() {
            declared_classes_block_table.insert(txn, class_hash, &block_number)?;
        }
    }
    Ok(())
}

fn write_deprecated_declared_classes_block<'env>(
    txn: &DbTransaction<'env, RW>,
    thin_state_diff: &ThinStateDiff,
    deprecated_declared_classes_block_table: &'env DeprecatedDeclaredClassesBlockTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    for class_hash in &thin_state_diff.deprecated_declared_classes {
        if deprecated_declared_classes_block_table.get(txn, class_hash)?.is_none() {
            deprecated_declared_classes_block_table.insert(txn, class_hash, &block_number)?;
        }
    }
    Ok(())
}

/// Build a reversed `ThinStateDiff` from pre-image tables for a given block.
/// The reversed diff contains the OLD values for each key changed in the block.
pub(crate) fn build_reversed_state_diff_from_preimages<'env, Mode: crate::db::TransactionKind>(
    txn: &DbTransaction<'env, Mode>,
    block_number: BlockNumber,
    storage_preimages_table: &'env StoragePreimagesTable<'env>,
    nonce_preimages_table: &'env NoncePreimagesTable<'env>,
    deployed_preimages_table: &'env DeployedContractPreimagesTable<'env>,
    class_preimages_table: &'env ClassPreimagesTable<'env>,
) -> StorageResult<ThinStateDiff> {
    use crate::db::table_types::DbCursorTrait;

    // Storage diffs: iterate storage_preimages for this block.
    let mut storage_diffs = indexmap::IndexMap::new();
    let mut cursor = storage_preimages_table.cursor(txn)?;
    let mut current =
        cursor.lower_bound(&((block_number, ContractAddress::default()), StorageKey::default()))?;
    while let Some(((bn, address), key)) = current.as_ref().map(|(k, _)| *k) {
        if bn != block_number {
            break;
        }
        let preimage = current.as_ref().map(|(_, v)| v.clone()).unwrap();
        let value = match preimage {
            PresencePrefixed(Some(v)) => v,
            PresencePrefixed(None) => Felt::default(),
        };
        storage_diffs.entry(address).or_insert_with(indexmap::IndexMap::new).insert(key, value);
        current = cursor.next()?;
    }

    // Deployed contracts: iterate deployed_contract_preimages for this block.
    let mut deployed_contracts = indexmap::IndexMap::new();
    let mut cursor = deployed_preimages_table.cursor(txn)?;
    let mut current = cursor.lower_bound(&(block_number, ContractAddress::default()))?;
    while let Some((bn, address)) = current.as_ref().map(|(k, _)| *k) {
        if bn != block_number {
            break;
        }
        let preimage = current.as_ref().map(|(_, v)| v.clone()).unwrap();
        let class_hash = match preimage {
            PresencePrefixed(Some(v)) => v,
            PresencePrefixed(None) => ClassHash::default(),
        };
        deployed_contracts.insert(address, class_hash);
        current = cursor.next()?;
    }

    // Nonces: iterate nonce_preimages for this block.
    let mut nonces = indexmap::IndexMap::new();
    let mut cursor = nonce_preimages_table.cursor(txn)?;
    let mut current = cursor.lower_bound(&(block_number, ContractAddress::default()))?;
    while let Some((bn, address)) = current.as_ref().map(|(k, _)| *k) {
        if bn != block_number {
            break;
        }
        let preimage = current.as_ref().map(|(_, v)| v.clone()).unwrap();
        let nonce = match preimage {
            PresencePrefixed(Some(v)) => v,
            PresencePrefixed(None) => Nonce::default(),
        };
        nonces.insert(address, nonce);
        current = cursor.next()?;
    }

    // Compiled class hashes: iterate class_preimages for this block.
    // Pre-images store (ClassHash, CompiledClassHash) — the CompiledClassHash is the NEW value
    // written by the block. For the reversed diff we need the OLD value. But class_preimages
    // doesn't store old values (classes are declared once). For the reversed diff, we use
    // CompiledClassHash::default() to indicate "class didn't exist before."
    let mut class_hash_to_compiled_class_hash = indexmap::IndexMap::new();
    let mut cursor = class_preimages_table.cursor(txn)?;
    let mut current = cursor.lower_bound(&(block_number, ClassHash::default()))?;
    while let Some((bn, class_hash)) = current.as_ref().map(|(k, _)| *k) {
        if bn != block_number {
            break;
        }
        class_hash_to_compiled_class_hash.insert(class_hash, CompiledClassHash::default());
        current = cursor.next()?;
    }

    Ok(ThinStateDiff {
        deployed_contracts,
        storage_diffs,
        class_hash_to_compiled_class_hash,
        nonces,
        deprecated_declared_classes: Vec::new(),
    })
}

/// Revert only the flat data tables (storage, nonces, deployed contracts, compiled class hashes)
/// using pre-image tables. Does NOT touch class visibility tables (declared_classes, casms,
/// deprecated_declared_classes) — use this during dual-write mode where the versioned revert
/// handles class visibility cleanup.
pub(crate) fn revert_flat_data_tables<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    tables: &'env FlatStateTables<'env>,
) -> StorageResult<()> {
    // Revert deployed contracts.
    revert_flat_simple_preimages(
        txn,
        block_number,
        &tables.flat_deployed_contracts,
        &tables.deployed_contract_preimages,
    )?;
    // Revert nonces.
    revert_flat_simple_preimages(txn, block_number, &tables.flat_nonces, &tables.nonce_preimages)?;
    // Revert storage diffs.
    revert_flat_storage(
        txn,
        block_number,
        &tables.flat_contract_storage,
        &tables.storage_preimages,
    )?;
    // Revert flat compiled class hashes (delete preimage entries, restore flat table).
    revert_flat_compiled_class_hashes(
        txn,
        block_number,
        &tables.flat_compiled_class_hash,
        &tables.class_preimages,
    )?;
    Ok(())
}

/// Revert flat state for a block using pre-image tables.
/// Must be called for the last block only (same constraint as archive revert).
pub(crate) fn revert_flat_state_diff<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    tables: &'env FlatStateRevertTables<'env>,
) -> StorageResult<()> {
    // ORDER MATTERS: class/deprecated-class cleanup must run BEFORE deployed contract revert,
    // because deprecated-class visibility cleanup needs the current (not-yet-reverted) deployed
    // class hashes from flat_deployed_contracts.

    // 1. Revert class declarations + visibility (Sierra classes, CASMs).
    revert_flat_classes(
        txn,
        block_number,
        &tables.base.flat_compiled_class_hash,
        &tables.base.class_preimages,
        &tables.base.declared_classes_block,
        &tables.declared_classes,
        &tables.compiled_classes,
    )?;
    // 2. Revert deprecated class visibility using deployed contract class hashes.
    revert_deprecated_classes(
        txn,
        block_number,
        &tables.base.flat_deployed_contracts,
        &tables.base.deployed_contract_preimages,
        &tables.deprecated_declared_classes,
        &tables.base.deprecated_declared_classes_block,
    )?;
    // 3. Revert deployed contracts.
    revert_flat_simple_preimages(
        txn,
        block_number,
        &tables.base.flat_deployed_contracts,
        &tables.base.deployed_contract_preimages,
    )?;
    // 4. Revert nonces.
    revert_flat_simple_preimages(
        txn,
        block_number,
        &tables.base.flat_nonces,
        &tables.base.nonce_preimages,
    )?;
    // 5. Revert storage diffs.
    revert_flat_storage(
        txn,
        block_number,
        &tables.base.flat_contract_storage,
        &tables.base.storage_preimages,
    )?;
    Ok(())
}

/// Revert storage diffs from `storage_preimages` (composite main key).
fn revert_flat_storage<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    flat_table: &'env FlatContractStorageTable<'env>,
    preimages_table: &'env StoragePreimagesTable<'env>,
) -> StorageResult<()> {
    use crate::db::table_types::DbCursorTrait;

    let mut cursor = preimages_table.cursor(txn)?;
    // Seek to the first entry for this block.
    let mut current =
        cursor.lower_bound(&((block_number, ContractAddress::default()), StorageKey::default()))?;
    while let Some(((bn, address), key)) = current.as_ref().map(|(k, _)| *k) {
        if bn != block_number {
            break;
        }
        let preimage = current.as_ref().map(|(_, v)| v.clone()).unwrap();
        match preimage {
            PresencePrefixed(Some(value)) => {
                flat_table.upsert(txn, &(address, key), &value)?;
            }
            PresencePrefixed(None) => {
                flat_table.delete(txn, &(address, key))?;
            }
        }
        preimages_table.delete(txn, &((bn, address), key))?;
        current = cursor.next()?;
    }
    Ok(())
}

/// Revert flat compiled class hashes using class preimage table.
fn revert_flat_compiled_class_hashes<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    flat_table: &'env FlatCompiledClassHashTable<'env>,
    preimages_table: &'env ClassPreimagesTable<'env>,
) -> StorageResult<()> {
    use crate::db::table_types::DbCursorTrait;

    let mut cursor = preimages_table.cursor(txn)?;
    let mut current = cursor.lower_bound(&(block_number, ClassHash::default()))?;
    while let Some((bn, class_hash)) = current.as_ref().map(|(k, _)| *k) {
        if bn != block_number {
            break;
        }
        flat_table.delete(txn, &class_hash)?;
        preimages_table.delete(txn, &(bn, class_hash))?;
        current = cursor.next()?;
    }
    Ok(())
}

/// Revert simple preimages (nonces, deployed contracts) with `(BlockNumber, K)` main key.
fn revert_flat_simple_preimages<'env, K, V>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    flat_table: &'env TableHandle<'env, K, NoVersionValueWrapper<V>, SimpleTable>,
    preimages_table: &'env TableHandle<
        'env,
        (BlockNumber, K),
        NoVersionValueWrapper<PresencePrefixed<V>>,
        CommonPrefix,
    >,
) -> StorageResult<()>
where
    K: crate::db::serialization::Key + std::fmt::Debug + Default,
    V: crate::db::serialization::StorageSerde + std::fmt::Debug + Clone,
    PresencePrefixed<V>: crate::db::serialization::StorageSerde + std::fmt::Debug,
    (BlockNumber, K): crate::db::serialization::Key + std::fmt::Debug,
{
    use crate::db::table_types::DbCursorTrait;

    let mut cursor = preimages_table.cursor(txn)?;
    let mut current = cursor.lower_bound(&(block_number, K::default()))?;
    while let Some((bn, entity_key)) = current.as_ref().map(|(k, _)| k.clone()) {
        if bn != block_number {
            break;
        }
        let preimage = current.as_ref().map(|(_, v)| v.clone()).unwrap();
        match preimage {
            PresencePrefixed(Some(value)) => {
                flat_table.upsert(txn, &entity_key, &value)?;
            }
            PresencePrefixed(None) => {
                flat_table.delete(txn, &entity_key)?;
            }
        }
        preimages_table.delete(txn, &(bn, entity_key))?;
        current = cursor.next()?;
    }
    Ok(())
}

/// Revert class declarations: delete from flat tables + class visibility tables.
fn revert_flat_classes<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    flat_compiled_hash_table: &'env FlatCompiledClassHashTable<'env>,
    class_preimages_table: &'env ClassPreimagesTable<'env>,
    declared_classes_block_table: &'env DeclaredClassesBlockTable<'env>,
    declared_classes_table: &'env DeclaredClassesTable<'env>,
    compiled_classes_table: &'env CompiledClassesTable<'env>,
) -> StorageResult<()> {
    use crate::db::table_types::DbCursorTrait;

    let mut cursor = class_preimages_table.cursor(txn)?;
    let mut current = cursor.lower_bound(&(block_number, ClassHash::default()))?;
    while let Some((bn, class_hash)) = current.as_ref().map(|(k, _)| *k) {
        if bn != block_number {
            break;
        }
        flat_compiled_hash_table.delete(txn, &class_hash)?;
        if declared_classes_block_table.get(txn, &class_hash)? == Some(block_number) {
            declared_classes_block_table.delete(txn, &class_hash)?;
            // Remove Sierra class visibility (mmap pointer becomes orphan but data is immutable).
            let _ = declared_classes_table.delete(txn, &class_hash);
            // Remove CASM visibility.
            let _ = compiled_classes_table.delete(txn, &class_hash);
        }
        class_preimages_table.delete(txn, &(bn, class_hash))?;
        current = cursor.next()?;
    }
    Ok(())
}

/// Revert deprecated class visibility for classes introduced by deployment.
fn revert_deprecated_classes<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    flat_deployed_table: &'env FlatDeployedContractsTable<'env>,
    deployed_preimages_table: &'env DeployedContractPreimagesTable<'env>,
    deprecated_declared_classes_table: &'env DeprecatedDeclaredClassesTable<'env>,
    deprecated_declared_classes_block_table: &'env DeprecatedDeclaredClassesBlockTable<'env>,
) -> StorageResult<()> {
    use crate::db::table_types::DbCursorTrait;

    // Build candidate set from two sources:
    // 1. deprecated_declared_classes_block entries for this block.
    // 2. Class hashes of contracts deployed in this block (from flat_deployed_contracts, which
    //    still holds the block's values before deployed-contract revert runs).

    let mut candidate_class_hashes = std::collections::HashSet::new();

    // Source 1: scan deprecated_declared_classes_block for entries with value == block_number.
    // This table is keyed by ClassHash with value BlockNumber. We scan the whole table (small)
    // to find classes first declared in the reverted block.
    {
        let mut block_cursor = deprecated_declared_classes_block_table.cursor(txn)?;
        let mut entry = block_cursor.lower_bound(&ClassHash::default())?;
        while let Some((class_hash, declared_block)) = entry {
            if declared_block == block_number {
                candidate_class_hashes.insert(class_hash);
            }
            entry = block_cursor.next()?;
        }
    }

    // Source 2: deployed contract class hashes from this block.
    let mut cursor = deployed_preimages_table.cursor(txn)?;
    let mut current = cursor.lower_bound(&(block_number, ContractAddress::default()))?;
    while let Some((bn, address)) = current.as_ref().map(|(k, _)| *k) {
        if bn != block_number {
            break;
        }
        // Read current class hash (before deployed-contract revert).
        if let Some(class_hash) = flat_deployed_table.get(txn, &address)? {
            candidate_class_hashes.insert(class_hash);
        }
        current = cursor.next()?;
    }

    // For each candidate, check if it was first stored in deprecated_declared_classes at this
    // block, and if so delete it.
    for class_hash in &candidate_class_hashes {
        if let Some(entry) = deprecated_declared_classes_table.get(txn, class_hash)? {
            if entry.block_number == block_number {
                deprecated_declared_classes_table.delete(txn, class_hash)?;
            }
        }
        if deprecated_declared_classes_block_table.get(txn, class_hash)? == Some(block_number) {
            deprecated_declared_classes_block_table.delete(txn, class_hash)?;
        }
    }

    Ok(())
}
