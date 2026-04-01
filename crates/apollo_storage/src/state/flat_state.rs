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
    TableHandle<'env, (ContractAddress, StorageKey), NoVersionValueWrapper<Felt>, SimpleTable>;
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

/// Write flat state + pre-images for a block's state diff.
pub(crate) fn write_flat_state_diff<'env>(
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    flat_storage_table: &'env FlatContractStorageTable<'env>,
    flat_nonces_table: &'env FlatNoncesTable<'env>,
    flat_deployed_table: &'env FlatDeployedContractsTable<'env>,
    flat_compiled_hash_table: &'env FlatCompiledClassHashTable<'env>,
    storage_preimages_table: &'env StoragePreimagesTable<'env>,
    nonce_preimages_table: &'env NoncePreimagesTable<'env>,
    deployed_preimages_table: &'env DeployedContractPreimagesTable<'env>,
    class_preimages_table: &'env ClassPreimagesTable<'env>,
    declared_classes_block_table: &'env DeclaredClassesBlockTable<'env>,
    deprecated_declared_classes_block_table: &'env DeprecatedDeclaredClassesBlockTable<'env>,
) -> StorageResult<()> {
    // 1. Deployed contracts + implicit nonce init (must come before explicit nonces).
    write_flat_deployed_contracts(
        txn,
        block_number,
        thin_state_diff,
        flat_deployed_table,
        deployed_preimages_table,
        flat_nonces_table,
        nonce_preimages_table,
    )?;

    // 2. Storage diffs.
    write_flat_storage_diffs(
        txn,
        block_number,
        thin_state_diff,
        flat_storage_table,
        storage_preimages_table,
    )?;

    // 3. Explicit nonces (must come after deployed contracts for coalescing).
    write_flat_nonces(
        txn,
        block_number,
        thin_state_diff,
        flat_nonces_table,
        nonce_preimages_table,
    )?;

    // 4. Class declarations.
    write_flat_compiled_class_hashes(
        txn,
        block_number,
        thin_state_diff,
        flat_compiled_hash_table,
        class_preimages_table,
        declared_classes_block_table,
    )?;

    // 5. Deprecated class declarations (same as archive — first declaration only).
    write_deprecated_declared_classes_block(
        txn,
        thin_state_diff,
        deprecated_declared_classes_block_table,
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
            let preimage = match current {
                Some(v) => PresencePrefixed::Present(v),
                None => PresencePrefixed::Absent,
            };
            preimages_table.insert(txn, &((block_number, *address), *key), &preimage)?;
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
        let preimage = match current_class {
            Some(v) => PresencePrefixed::Present(v),
            None => PresencePrefixed::Absent,
        };
        deployed_preimages_table.insert(txn, &(block_number, *address), &preimage)?;
        flat_deployed_table.upsert(txn, address, class_hash)?;

        // Implicit nonce initialization for new deployments.
        if flat_nonces_table.get(txn, address)?.is_none() {
            nonce_preimages_table.insert(
                txn,
                &(block_number, *address),
                &PresencePrefixed::Absent,
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
            let preimage = match current {
                Some(v) => PresencePrefixed::Present(v),
                None => PresencePrefixed::Absent,
            };
            nonce_preimages_table.insert(txn, &(block_number, *address), &preimage)?;
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

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use starknet_api::block::BlockNumber;
    use starknet_api::core::{ClassHash, ContractAddress, Nonce};
    use starknet_api::felt;
    use starknet_api::state::ThinStateDiff;

    use super::write_flat_state_diff;
    use crate::db::table_types::Table;
    use crate::state::presence_prefixed::PresencePrefixed;
    use crate::test_utils::get_test_storage;

    /// Helper: opens all flat/preimage tables and calls write_flat_state_diff.
    fn write_flat_diff(
        writer: &mut crate::StorageWriter,
        block_number: BlockNumber,
        diff: &ThinStateDiff,
    ) {
        let txn = writer.begin_rw_txn().unwrap();
        let flat_storage = txn.open_table(&txn.tables.flat_contract_storage).unwrap();
        let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();
        let flat_deployed = txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
        let flat_compiled = txn.open_table(&txn.tables.flat_compiled_class_hash).unwrap();
        let storage_preimages = txn.open_table(&txn.tables.storage_preimages).unwrap();
        let nonce_preimages = txn.open_table(&txn.tables.nonce_preimages).unwrap();
        let deployed_preimages = txn.open_table(&txn.tables.deployed_contract_preimages).unwrap();
        let class_preimages = txn.open_table(&txn.tables.class_preimages).unwrap();
        let declared_classes_block = txn.open_table(&txn.tables.declared_classes_block).unwrap();
        let deprecated_declared_classes_block =
            txn.open_table(&txn.tables.deprecated_declared_classes_block).unwrap();
        write_flat_state_diff(
            &txn.txn,
            block_number,
            diff,
            &flat_storage,
            &flat_nonces,
            &flat_deployed,
            &flat_compiled,
            &storage_preimages,
            &nonce_preimages,
            &deployed_preimages,
            &class_preimages,
            &declared_classes_block,
            &deprecated_declared_classes_block,
        )
        .unwrap();
        txn.commit().unwrap();
    }

    #[test]
    fn flat_append_storage_diffs() {
        let ((reader, mut writer), _temp_dir) = get_test_storage();
        let address = ContractAddress::from(1_u128);
        let key = 2u128.into();
        let value = felt!("0x42");
        let diff = ThinStateDiff {
            storage_diffs: IndexMap::from([(address, IndexMap::from([(key, value)]))]),
            ..Default::default()
        };

        write_flat_diff(&mut writer, BlockNumber(0), &diff);

        let txn = reader.begin_ro_txn().unwrap();
        let flat_table = txn.open_table(&txn.tables.flat_contract_storage).unwrap();
        assert_eq!(flat_table.get(&txn.txn, &(address, key)).unwrap(), Some(value));

        let preimages = txn.open_table(&txn.tables.storage_preimages).unwrap();
        let preimage = preimages.get(&txn.txn, &((BlockNumber(0), address), key)).unwrap();
        assert!(matches!(preimage, Some(PresencePrefixed::Absent)));
    }

    #[test]
    fn flat_deploy_initializes_nonce() {
        let ((reader, mut writer), _temp_dir) = get_test_storage();
        let address = ContractAddress::from(1_u128);
        let class_hash = ClassHash(felt!("0x10"));
        let diff = ThinStateDiff {
            deployed_contracts: IndexMap::from([(address, class_hash)]),
            ..Default::default()
        };

        write_flat_diff(&mut writer, BlockNumber(0), &diff);

        let txn = reader.begin_ro_txn().unwrap();
        let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();
        assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), Some(Nonce::default()));

        let flat_deployed = txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
        assert_eq!(flat_deployed.get(&txn.txn, &address).unwrap(), Some(class_hash));
    }

    #[test]
    fn flat_deploy_with_explicit_nonce() {
        let ((reader, mut writer), _temp_dir) = get_test_storage();
        let address = ContractAddress::from(1_u128);
        let class_hash = ClassHash(felt!("0x10"));
        let explicit_nonce = Nonce(felt!("0x5"));
        let diff = ThinStateDiff {
            deployed_contracts: IndexMap::from([(address, class_hash)]),
            nonces: IndexMap::from([(address, explicit_nonce)]),
            ..Default::default()
        };

        write_flat_diff(&mut writer, BlockNumber(0), &diff);

        let txn = reader.begin_ro_txn().unwrap();
        let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();
        assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), Some(explicit_nonce));

        let preimages = txn.open_table(&txn.tables.nonce_preimages).unwrap();
        let preimage = preimages.get(&txn.txn, &(BlockNumber(0), address)).unwrap();
        assert!(matches!(preimage, Some(PresencePrefixed::Absent)));
    }

    #[test]
    fn flat_class_declaration() {
        let ((reader, mut writer), _temp_dir) = get_test_storage();
        let class_hash = ClassHash(felt!("0x20"));
        let compiled = starknet_api::core::CompiledClassHash(felt!("0x30"));
        let diff = ThinStateDiff {
            class_hash_to_compiled_class_hash: IndexMap::from([(class_hash, compiled)]),
            ..Default::default()
        };

        write_flat_diff(&mut writer, BlockNumber(0), &diff);

        let txn = reader.begin_ro_txn().unwrap();
        let flat_compiled = txn.open_table(&txn.tables.flat_compiled_class_hash).unwrap();
        assert_eq!(flat_compiled.get(&txn.txn, &class_hash).unwrap(), Some(compiled));

        let class_preimages = txn.open_table(&txn.tables.class_preimages).unwrap();
        assert_eq!(
            class_preimages.get(&txn.txn, &(BlockNumber(0), class_hash)).unwrap(),
            Some(compiled)
        );

        let declared_block = txn.open_table(&txn.tables.declared_classes_block).unwrap();
        assert_eq!(declared_block.get(&txn.txn, &class_hash).unwrap(), Some(BlockNumber(0)));
    }
}
