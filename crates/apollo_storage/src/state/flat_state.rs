//! Flat state write and revert functions for sequencer mode.
//!
//! In sequencer mode, state is stored in flat tables (current value only) with pre-image tables
//! for revert support. This module contains the write functions that replace the versioned-table
//! writes used in archive mode.

use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_types_core::felt::Felt;

use super::presence_prefixed::PresencePrefixed;
use crate::db::serialization::NoVersionValueWrapper;
use crate::db::table_types::{CommonPrefix, SimpleTable, Table};
use crate::db::{DbTransaction, TableHandle, RW};
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

/// All flat state and pre-image table handles needed for write and revert operations.
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
