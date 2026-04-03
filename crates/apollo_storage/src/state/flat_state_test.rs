use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::felt;
use starknet_api::state::ThinStateDiff;

use super::flat_state::{write_flat_state_diff, FlatStateTables};
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
    let tables = FlatStateTables {
        flat_contract_storage: txn.open_table(&txn.tables.flat_contract_storage).unwrap(),
        flat_nonces: txn.open_table(&txn.tables.flat_nonces).unwrap(),
        flat_deployed_contracts: txn.open_table(&txn.tables.flat_deployed_contracts).unwrap(),
        flat_compiled_class_hash: txn.open_table(&txn.tables.flat_compiled_class_hash).unwrap(),
        storage_preimages: txn.open_table(&txn.tables.storage_preimages).unwrap(),
        nonce_preimages: txn.open_table(&txn.tables.nonce_preimages).unwrap(),
        deployed_contract_preimages: txn
            .open_table(&txn.tables.deployed_contract_preimages)
            .unwrap(),
        class_preimages: txn.open_table(&txn.tables.class_preimages).unwrap(),
        declared_classes_block: txn.open_table(&txn.tables.declared_classes_block).unwrap(),
        deprecated_declared_classes_block: txn
            .open_table(&txn.tables.deprecated_declared_classes_block)
            .unwrap(),
    };
    write_flat_state_diff(&txn.txn, block_number, diff, &tables).unwrap();
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
    assert_eq!(preimage, Some(PresencePrefixed::absent()));
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
    assert_eq!(preimage, Some(PresencePrefixed::absent()));
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
