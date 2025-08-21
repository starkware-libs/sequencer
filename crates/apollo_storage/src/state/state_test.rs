use apollo_test_utils::get_test_state_diff;
use assert_matches::assert_matches;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::{indexmap, IndexMap};
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::StarkHash;
use starknet_api::state::{SierraContractClass, StateNumber, ThinStateDiff};
use starknet_api::{class_hash, compiled_class_hash, contract_address, felt, storage_key};
use starknet_types_core::felt::Felt;

use crate::class::{ClassStorageReader, ClassStorageWriter};
use crate::compiled_class::{CasmStorageReader, CasmStorageWriter};
use crate::state::{StateStorageReader, StateStorageWriter};
use crate::test_utils::get_test_storage;
use crate::StorageWriter;

#[test]
fn get_class_definition_at() {
    // Deprecated classes.
    let dc0 = class_hash!("0x00");
    let dc1 = class_hash!("0x01");
    let dep_class = DeprecatedContractClass::default();
    // New classes.
    let nc0 = class_hash!("0x10");
    let nc1 = class_hash!("0x11");
    let new_class = SierraContractClass::default();
    let compiled_class_hash = compiled_class_hash!(1_u8);
    let diff0 = ThinStateDiff {
        deprecated_declared_classes: vec![dc0, dc1],
        declared_classes: IndexMap::from([(nc0, compiled_class_hash)]),
        ..Default::default()
    };
    let diff1 = ThinStateDiff {
        deprecated_declared_classes: vec![dc0],
        declared_classes: IndexMap::from([(nc1, compiled_class_hash)]),
        ..Default::default()
    };

    let ((_, mut writer), _temp_dir) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff0).unwrap();
    txn = txn.append_state_diff(BlockNumber(1), diff1).unwrap();
    txn = txn
        .append_classes(
            BlockNumber(0),
            &[(nc0, &new_class)],
            &[(dc0, &dep_class), (dc1, &dep_class)],
        )
        .unwrap();
    txn = txn.append_classes(BlockNumber(1), &[(nc1, &new_class)], &[(dc0, &dep_class)]).unwrap();
    txn.commit().unwrap();

    // State numbers.
    let state0 = StateNumber::right_before_block(BlockNumber(0));
    let state1 = StateNumber::right_before_block(BlockNumber(1));
    let state2 = StateNumber::right_before_block(BlockNumber(2));

    // Deprecated Classes Test

    let txn = writer.begin_rw_txn().unwrap();
    let statetxn = txn.get_state_reader().unwrap();

    // Class0.
    assert!(statetxn.get_deprecated_class_definition_at(state0, &dc0).unwrap().is_none());
    assert!(statetxn.get_deprecated_class_definition_at(state1, &dc0).unwrap().is_some());
    assert!(statetxn.get_deprecated_class_definition_at(state2, &dc0).unwrap().is_some());
    assert_eq!(
        statetxn.get_deprecated_class_definition_block_number(&dc0).unwrap(),
        Some(BlockNumber(0))
    );

    // Class1.
    assert!(statetxn.get_deprecated_class_definition_at(state0, &dc1).unwrap().is_none());
    assert!(statetxn.get_deprecated_class_definition_at(state1, &dc1).unwrap().is_some());
    assert!(statetxn.get_deprecated_class_definition_at(state2, &dc1).unwrap().is_some());
    assert_eq!(
        statetxn.get_deprecated_class_definition_block_number(&dc0).unwrap(),
        Some(BlockNumber(0))
    );

    // New Classes Test
    drop(txn);
    let txn = writer.begin_rw_txn().unwrap();
    let statetxn = txn.get_state_reader().unwrap();

    // Class0.
    assert!(statetxn.get_class_definition_at(state0, &nc0).unwrap().is_none());
    assert!(statetxn.get_class_definition_at(state1, &nc0).unwrap().is_some());
    assert!(statetxn.get_class_definition_at(state2, &nc0).unwrap().is_some());
    assert_eq!(statetxn.get_class_definition_block_number(&nc0).unwrap(), Some(BlockNumber(0)));

    // Class1.
    assert!(statetxn.get_class_definition_at(state0, &nc1).unwrap().is_none());
    assert!(statetxn.get_class_definition_at(state1, &nc1).unwrap().is_none());
    assert!(statetxn.get_class_definition_at(state2, &nc1).unwrap().is_some());
    assert_eq!(statetxn.get_class_definition_block_number(&nc1).unwrap(), Some(BlockNumber(1)));
}

#[test]
fn append_state_diff_replaced_classes() {
    let contract_0 = contract_address!("0x00");
    let contract_1 = contract_address!("0x01");
    let compiled_class_hash = compiled_class_hash!(2_u8);
    let hash_0 = class_hash!("0x10");
    let hash_1 = class_hash!("0x11");
    let diff0 = ThinStateDiff {
        deployed_contracts: IndexMap::from([(contract_0, hash_0), (contract_1, hash_1)]),
        deprecated_declared_classes: vec![hash_0],
        declared_classes: IndexMap::from([(hash_1, compiled_class_hash)]),
        ..Default::default()
    };
    // Replacements between different class types (cairo0 and cairo1).
    let diff1 = ThinStateDiff {
        deployed_contracts: IndexMap::from([(contract_0, hash_1), (contract_1, hash_0)]),
        ..Default::default()
    };
    // Replace to class that was declared in the same block.
    let hash_2 = class_hash!("0x12");
    let diff2 = ThinStateDiff {
        declared_classes: IndexMap::from([(hash_2, compiled_class_hash)]),
        deployed_contracts: IndexMap::from([(contract_1, hash_2)]),
        ..Default::default()
    };

    let ((_, mut writer), _temp_dir) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff0).unwrap();
    txn = txn.append_state_diff(BlockNumber(1), diff1).unwrap();
    txn = txn.append_state_diff(BlockNumber(2), diff2).unwrap();
    txn.commit().unwrap();

    // State numbers.
    let state0 = StateNumber::right_before_block(BlockNumber(0));
    let state1 = StateNumber::right_before_block(BlockNumber(1));
    let state2 = StateNumber::right_before_block(BlockNumber(2));
    let state3 = StateNumber::right_before_block(BlockNumber(3));

    let txn = writer.begin_rw_txn().unwrap();
    let statetxn = txn.get_state_reader().unwrap();

    // Contract_0
    assert_eq!(statetxn.get_class_hash_at(state0, &contract_0).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state1, &contract_0).unwrap(), Some(hash_0));
    assert_eq!(statetxn.get_class_hash_at(state2, &contract_0).unwrap(), Some(hash_1));
    assert_eq!(statetxn.get_class_hash_at(state3, &contract_0).unwrap(), Some(hash_1));

    // Contract_1
    assert_eq!(statetxn.get_class_hash_at(state0, &contract_1).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state1, &contract_1).unwrap(), Some(hash_1));
    assert_eq!(statetxn.get_class_hash_at(state2, &contract_1).unwrap(), Some(hash_0));
    assert_eq!(statetxn.get_class_hash_at(state3, &contract_1).unwrap(), Some(hash_2));
}

#[test]
fn append_state_diff() {
    let c0 = contract_address!("0x11");
    let c1 = contract_address!("0x12");
    let c2 = contract_address!("0x13");
    let c3 = contract_address!("0x14");
    let cl0 = class_hash!("0x4");
    let c_cls_0 = compiled_class_hash!(123);
    let cl1 = class_hash!("0x5");
    let c_cls_1 = compiled_class_hash!(456);
    let key0 = storage_key!("0x1001");
    let key1 = storage_key!("0x101");
    let diff0 = ThinStateDiff {
        deployed_contracts: IndexMap::from([(c0, cl0), (c1, cl1)]),
        storage_diffs: IndexMap::from([
            (c0, IndexMap::from([(key0, felt!("0x200")), (key1, felt!("0x201"))])),
            (c1, IndexMap::new()),
        ]),
        deprecated_declared_classes: vec![cl0],
        declared_classes: IndexMap::from([(cl0, c_cls_0), (cl1, c_cls_1)]),
        nonces: IndexMap::from([(c0, Nonce(StarkHash::from(1_u8)))]),
    };
    let diff1 = ThinStateDiff {
        deployed_contracts: IndexMap::from([(c2, cl0), (c0, cl1)]),
        storage_diffs: IndexMap::from([
            (c0, IndexMap::from([(key0, felt!("0x300")), (key1, felt!("0x0"))])),
            (c1, IndexMap::from([(key0, felt!("0x0"))])),
        ]),
        deprecated_declared_classes: vec![cl0],
        declared_classes: indexmap! {},
        nonces: IndexMap::from([
            (c0, Nonce(StarkHash::from(2_u8))),
            (c1, Nonce(StarkHash::from(1_u8))),
            (c2, Nonce(StarkHash::from(1_u8))),
        ]),
    };

    let ((_, mut writer), _temp_dir) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    assert_eq!(txn.get_state_diff(BlockNumber(0)).unwrap(), None);
    assert_eq!(txn.get_state_diff(BlockNumber(1)).unwrap(), None);
    txn = txn.append_state_diff(BlockNumber(0), diff0.clone()).unwrap();
    assert_eq!(txn.get_state_diff(BlockNumber(0)).unwrap().unwrap(), diff0);
    assert_eq!(txn.get_state_diff(BlockNumber(1)).unwrap(), None);
    txn = txn.append_state_diff(BlockNumber(1), diff1.clone()).unwrap();

    txn.commit().unwrap();

    let txn = writer.begin_rw_txn().unwrap();
    assert_eq!(txn.get_state_diff(BlockNumber(0)).unwrap().unwrap(), diff0);
    assert_eq!(txn.get_state_diff(BlockNumber(1)).unwrap().unwrap(), diff1);

    let statetxn = txn.get_state_reader().unwrap();

    // State numbers.
    let state0 = StateNumber::right_before_block(BlockNumber(0));
    let state1 = StateNumber::right_before_block(BlockNumber(1));
    let state2 = StateNumber::right_before_block(BlockNumber(2));

    // Contract0.
    assert_eq!(statetxn.get_class_hash_at(state0, &c0).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c0).unwrap(), Some(cl0));
    assert_eq!(statetxn.get_class_hash_at(state2, &c0).unwrap(), Some(cl1));
    assert_eq!(statetxn.get_nonce_at(state0, &c0).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state1, &c0).unwrap(), Some(Nonce(StarkHash::from(1_u8))));
    assert_eq!(statetxn.get_nonce_at(state2, &c0).unwrap(), Some(Nonce(StarkHash::from(2_u8))));

    // Contract1.
    assert_eq!(statetxn.get_class_hash_at(state0, &c1).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c1).unwrap(), Some(cl1));
    assert_eq!(statetxn.get_class_hash_at(state2, &c1).unwrap(), Some(cl1));
    assert_eq!(statetxn.get_nonce_at(state0, &c1).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state1, &c1).unwrap(), Some(Nonce::default()));
    assert_eq!(statetxn.get_nonce_at(state2, &c1).unwrap(), Some(Nonce(StarkHash::from(1_u8))));

    // Contract2.
    assert_eq!(statetxn.get_class_hash_at(state0, &c2).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c2).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state2, &c2).unwrap(), Some(cl0));
    assert_eq!(statetxn.get_nonce_at(state0, &c2).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state1, &c2).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state2, &c2).unwrap(), Some(Nonce(StarkHash::from(1_u8))));

    // Contract3.
    assert_eq!(statetxn.get_class_hash_at(state0, &c3).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c3).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state2, &c3).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state0, &c3).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state1, &c3).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state2, &c3).unwrap(), None);

    // Storage at key0.
    assert_eq!(statetxn.get_storage_at(state0, &c0, &key0).unwrap(), felt!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c0, &key0).unwrap(), felt!("0x200"));
    assert_eq!(statetxn.get_storage_at(state2, &c0, &key0).unwrap(), felt!("0x300"));

    // Storage at key1.
    assert_eq!(statetxn.get_storage_at(state0, &c0, &key1).unwrap(), felt!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c0, &key1).unwrap(), felt!("0x201"));
    assert_eq!(statetxn.get_storage_at(state2, &c0, &key1).unwrap(), felt!("0x0"));

    // Storage at key2.
    assert_eq!(statetxn.get_storage_at(state0, &c1, &key0).unwrap(), felt!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c1, &key0).unwrap(), felt!("0x0"));
    assert_eq!(statetxn.get_storage_at(state2, &c1, &key0).unwrap(), felt!("0x0"));

    // Class 0
    assert_eq!(statetxn.get_compiled_class_hash_at(state0, &cl0).unwrap(), None);
    assert_eq!(statetxn.get_compiled_class_hash_at(state1, &cl0).unwrap(), Some(c_cls_0));
    assert_eq!(statetxn.get_compiled_class_hash_at(state2, &cl0).unwrap(), Some(c_cls_0));

    // Class 1
    assert_eq!(statetxn.get_compiled_class_hash_at(state0, &cl1).unwrap(), None);
    assert_eq!(statetxn.get_compiled_class_hash_at(state1, &cl1).unwrap(), Some(c_cls_1));
    assert_eq!(statetxn.get_compiled_class_hash_at(state1, &cl1).unwrap(), Some(c_cls_1));
}

#[test]
fn test_update_compiled_class_marker() {
    let ((_, mut writer), _temp_dir) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    // Append an empty state diff.
    txn = txn.append_state_diff(BlockNumber(0), ThinStateDiff::default()).unwrap();
    assert_eq!(txn.get_compiled_class_marker().unwrap(), BlockNumber(1));
}

#[test]
fn test_get_class_after_append_thin_state_diff() {
    const CLASS_HASH: ClassHash = ClassHash(StarkHash::ZERO);
    const DEPRECATED_CLASS_HASH: ClassHash = ClassHash(StarkHash::ONE);

    let ((_, mut writer), _temp_dir) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    // Append an empty state diff.
    txn = txn
        .append_state_diff(
            BlockNumber(0),
            ThinStateDiff {
                declared_classes: indexmap! { CLASS_HASH => compiled_class_hash!(3_u8) },
                deprecated_declared_classes: vec![DEPRECATED_CLASS_HASH],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(txn.get_class_marker().unwrap(), BlockNumber(0));

    let state_reader = txn.get_state_reader().unwrap();
    let state_number = StateNumber::unchecked_right_after_block(BlockNumber(0));

    assert_eq!(
        state_reader.get_class_definition_block_number(&CLASS_HASH).unwrap(),
        Some(BlockNumber(0))
    );
    assert_eq!(
        state_reader.get_deprecated_class_definition_block_number(&DEPRECATED_CLASS_HASH).unwrap(),
        Some(BlockNumber(0)),
    );
    assert!(state_reader.get_class_definition_at(state_number, &CLASS_HASH).unwrap().is_none());
    assert!(
        state_reader
            .get_deprecated_class_definition_at(state_number, &DEPRECATED_CLASS_HASH)
            .unwrap()
            .is_none()
    );
}

#[test]
fn revert_non_existing_state_diff() {
    let ((_, mut writer), _temp_dir) = get_test_storage();

    let block_number = BlockNumber(5);
    let (_, deleted_data) = writer.begin_rw_txn().unwrap().revert_state_diff(block_number).unwrap();
    assert!(deleted_data.is_none());
}

#[tokio::test]
async fn revert_last_state_diff_success() {
    let ((_, mut writer), _temp_dir) = get_test_storage();
    let state_diff = get_test_state_diff().into();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), state_diff)
        .unwrap()
        .commit()
        .unwrap();

    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    txn.commit().unwrap();
}

#[tokio::test]
async fn revert_old_state_diff_fails() {
    let ((_, mut writer), _temp_dir) = get_test_storage();
    append_2_state_diffs(&mut writer);
    let (_, deleted_data) =
        writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    assert!(deleted_data.is_none());
}

#[tokio::test]
async fn revert_state_diff_updates_marker() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    append_2_state_diffs(&mut writer);

    // Verify that the state marker before revert is 2.
    assert_eq!(reader.begin_ro_txn().unwrap().get_state_marker().unwrap(), BlockNumber(2));

    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap();
    txn.commit().unwrap();
    assert_eq!(reader.begin_ro_txn().unwrap().get_state_marker().unwrap(), BlockNumber(1));
}

#[tokio::test]
async fn get_reverted_state_diff_returns_none() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    append_2_state_diffs(&mut writer);

    // Verify that we can get block 1's state before the revert.
    assert!(reader.begin_ro_txn().unwrap().get_state_diff(BlockNumber(1)).unwrap().is_some());

    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap();
    txn.commit().unwrap();
    assert!(reader.begin_ro_txn().unwrap().get_state_diff(BlockNumber(1)).unwrap().is_none());
}

fn append_2_state_diffs(writer: &mut StorageWriter) {
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), ThinStateDiff::default())
        .unwrap()
        .append_state_diff(BlockNumber(1), ThinStateDiff::default())
        .unwrap()
        .commit()
        .unwrap();
}

#[test]
fn revert_doesnt_delete_previously_declared_classes() {
    // Append 2 state diffs that use the same declared class.
    let c0 = contract_address!("0x11");
    let cl0 = class_hash!("0x4");
    let c_cls0 = DeprecatedContractClass::default();
    let diff0 = ThinStateDiff {
        deployed_contracts: IndexMap::from([(c0, cl0)]),
        storage_diffs: IndexMap::new(),
        deprecated_declared_classes: vec![cl0],
        declared_classes: indexmap! {},
        nonces: IndexMap::from([(c0, Nonce(StarkHash::from(1_u8)))]),
    };

    let c1 = contract_address!("0x12");
    let diff1 = ThinStateDiff {
        deployed_contracts: IndexMap::from([(c1, cl0)]),
        storage_diffs: IndexMap::new(),
        deprecated_declared_classes: vec![cl0],
        declared_classes: indexmap! {},
        nonces: IndexMap::from([(c1, Nonce(StarkHash::from(2_u8)))]),
    };

    let ((reader, mut writer), _temp_dir) = get_test_storage();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .append_classes(BlockNumber(0), &[], &[(cl0, &c_cls0)])
        .unwrap()
        .append_state_diff(BlockNumber(1), diff1)
        .unwrap()
        .append_classes(BlockNumber(1), &[], &[(cl0, &c_cls0)])
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    assert_eq!(
        state_reader.get_deprecated_class_definition_block_number(&cl0).unwrap(),
        Some(BlockNumber(0))
    );

    // Assert that reverting diff 1 doesn't delete declared class from diff 0.
    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap();
    txn.commit().unwrap();
    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let declared_class = state_reader
        .get_deprecated_class_definition_at(
            StateNumber::unchecked_right_after_block(BlockNumber(0)),
            &cl0,
        )
        .unwrap();
    assert!(declared_class.is_some());
    assert_eq!(
        state_reader.get_deprecated_class_definition_block_number(&cl0).unwrap(),
        Some(BlockNumber(0))
    );

    // Assert that reverting diff 0 deletes the declared class.
    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    txn.commit().unwrap();
    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let declared_class = state_reader
        .get_deprecated_class_definition_at(
            StateNumber::unchecked_right_after_block(BlockNumber(0)),
            &cl0,
        )
        .unwrap();
    assert!(declared_class.is_none());
    assert_eq!(state_reader.get_deprecated_class_definition_block_number(&cl0).unwrap(), None);
}

#[test]
fn revert_state() {
    let (mut state_diff0, classes0, deprecated_classes0) =
        ThinStateDiff::from_state_diff(get_test_state_diff());
    let (contract0, class0) = state_diff0.deployed_contracts.first().unwrap();
    // Change nonce to non-zero value to make sure it isn't overwritten when replacing the class.
    let nonce0 = Nonce(Felt::from(7_u8));
    state_diff0.nonces = IndexMap::from([(*contract0, nonce0)]);

    // Create another state diff, deploying new contracts and changing the state and the class hash
    // of the contract deployed in state0.
    let contract1 = contract_address!("0x1");
    let contract2 = contract_address!("0x2");
    let class1 = class_hash!("0x11");
    let class2 = class_hash!("0x22");
    let compiled_class_hash_2 = compiled_class_hash!(456_u16);
    let compiled_class2 = CasmContractClass {
        prime: Default::default(),
        compiler_version: Default::default(),
        bytecode: Default::default(),
        bytecode_segment_lengths: Default::default(),
        hints: Default::default(),
        pythonic_hints: Default::default(),
        entry_points_by_type: Default::default(),
    };
    let updated_storage_key = storage_key!("0x1");
    let new_data = Felt::from(1_u8);
    let updated_storage = IndexMap::from([(updated_storage_key, new_data)]);
    let nonce1 = Nonce(Felt::from(111_u8));
    let state_diff1 = ThinStateDiff {
        deployed_contracts: IndexMap::from([
            (contract1, class1),
            (contract2, class2),
            (*contract0, class1),
        ]),
        storage_diffs: IndexMap::from([(*contract0, updated_storage)]),
        deprecated_declared_classes: vec![class1],
        declared_classes: IndexMap::from([(class2, compiled_class_hash_2)]),
        nonces: IndexMap::from([(contract1, nonce1)]),
    };

    let ((reader, mut writer), _temp_dir) = get_test_storage();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), state_diff0.clone())
        .unwrap()
        .append_state_diff(BlockNumber(1), state_diff1.clone())
        .unwrap()
        .append_classes(
            BlockNumber(0),
            &classes0.iter().map(|(class_hash, class)| (*class_hash, class)).collect::<Vec<_>>(),
            &deprecated_classes0
                .iter()
                .map(|(class_hash, deprecated_class)| (*class_hash, deprecated_class))
                .collect::<Vec<_>>(),
        )
        .unwrap()
        .append_classes(
            BlockNumber(1),
            &[(class2, &SierraContractClass::default())],
            &[(class1, &DeprecatedContractClass::default())],
        )
        .unwrap()
        .append_casm(&class2, &compiled_class2)
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    assert_eq!(txn.get_state_marker().unwrap(), BlockNumber(2));
    assert!(txn.get_state_diff(BlockNumber(1)).unwrap().is_some());

    let state_reader = txn.get_state_reader().unwrap();
    let state_number = StateNumber::unchecked_right_after_block(BlockNumber(1));
    assert_eq!(state_reader.get_class_hash_at(state_number, contract0).unwrap().unwrap(), class1);
    assert_eq!(state_reader.get_class_hash_at(state_number, &contract1).unwrap().unwrap(), class1);
    assert_eq!(state_reader.get_class_hash_at(state_number, &contract2).unwrap().unwrap(), class2);
    assert_eq!(state_reader.get_nonce_at(state_number, contract0).unwrap().unwrap(), nonce0);
    assert_eq!(state_reader.get_nonce_at(state_number, &contract1).unwrap().unwrap(), nonce1);
    assert_eq!(
        state_reader.get_storage_at(state_number, contract0, &updated_storage_key).unwrap(),
        new_data
    );
    assert_eq!(
        state_reader.get_compiled_class_hash_at(state_number, &class2).unwrap().unwrap(),
        compiled_class_hash_2
    );

    let block_number = BlockNumber(1);
    let (txn, deleted_data) =
        writer.begin_rw_txn().unwrap().revert_state_diff(block_number).unwrap();
    txn.commit().unwrap();

    let expected_deleted_class_hashes = vec![class2];
    let expected_deleted_deprecated_class_hashes = vec![class1];
    let expected_deleted_deprecated_classes =
        IndexMap::from([(class1, DeprecatedContractClass::default())]);
    let expected_deleted_classes = IndexMap::from([(class2, SierraContractClass::default())]);
    let expected_deleted_compiled_classes = IndexMap::from([(
        class2,
        CasmContractClass {
            prime: Default::default(),
            compiler_version: Default::default(),
            bytecode: Default::default(),
            bytecode_segment_lengths: Default::default(),
            hints: Default::default(),
            pythonic_hints: Default::default(),
            entry_points_by_type: Default::default(),
        },
    )]);
    assert_matches!(
        deleted_data,
        Some((thin_state_diff, class_hashes, class_definitions, deprecated_class_hashes, deprecated_class_definitions, compiled_classes))
        if thin_state_diff == state_diff1
        && class_hashes == expected_deleted_class_hashes
        && class_definitions == expected_deleted_classes
        && deprecated_class_hashes == expected_deleted_deprecated_class_hashes
        && deprecated_class_definitions == expected_deleted_deprecated_classes
        && compiled_classes == expected_deleted_compiled_classes
    );

    let txn = reader.begin_ro_txn().unwrap();
    assert_eq!(txn.get_state_marker().unwrap(), BlockNumber(1));
    assert!(txn.get_state_diff(BlockNumber(1)).unwrap().is_none());

    let state_reader = txn.get_state_reader().unwrap();
    let state_number = StateNumber::unchecked_right_after_block(BlockNumber(0));
    assert_eq!(state_reader.get_class_hash_at(state_number, contract0).unwrap().unwrap(), *class0);
    assert!(state_reader.get_class_hash_at(state_number, &contract1).unwrap().is_none());
    assert!(state_reader.get_class_hash_at(state_number, &contract2).unwrap().is_none());
    assert_eq!(state_reader.get_nonce_at(state_number, contract0).unwrap().unwrap(), nonce0);
    assert!(state_reader.get_nonce_at(state_number, &contract1).unwrap().is_none());
    assert!(state_reader.get_nonce_at(state_number, &contract2).unwrap().is_none());
    assert!(state_reader.get_compiled_class_hash_at(state_number, &class2).unwrap().is_none());
    assert_eq!(
        state_reader.get_storage_at(state_number, contract0, &updated_storage_key).unwrap(),
        Felt::ZERO
    );
    assert!(txn.get_casm(&class2).unwrap().is_none());
}

#[test]
fn get_nonce_key_serialization() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let contract_address = contract_address!("0x11");

    for block_number in 0..(1 << 8) + 1 {
        let state_diff = ThinStateDiff {
            deployed_contracts: IndexMap::new(),
            storage_diffs: IndexMap::new(),
            declared_classes: IndexMap::new(),
            deprecated_declared_classes: Vec::new(),
            nonces: IndexMap::from([(
                contract_address,
                Nonce(StarkHash::from(u128::from(block_number) + 1)),
            )]),
        };

        writer
            .begin_rw_txn()
            .unwrap()
            .append_state_diff(BlockNumber(block_number), state_diff)
            .unwrap()
            .commit()
            .unwrap();
    }

    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    // No nonce in genesis.
    assert_eq!(
        state_reader
            .get_nonce_at(StateNumber::right_before_block(BlockNumber(0)), &contract_address)
            .unwrap(),
        None
    );

    for block_number in 1..(1 << 8) + 1 {
        println!("{block_number:?}");
        let nonce = state_reader
            .get_nonce_at(
                StateNumber::right_before_block(BlockNumber(block_number)),
                &contract_address,
            )
            .unwrap();
        println!("{nonce:?}");
        let nonce = nonce.unwrap();

        assert_eq!(nonce, Nonce(StarkHash::from(u128::from(block_number))));
    }
}

#[test]
fn replace_class() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let contract_address = contract_address!("0x0");

    let class_hash0 = class_hash!("0x0");
    let state_diff1 = ThinStateDiff {
        deployed_contracts: indexmap! {
            contract_address => class_hash0
        },
        storage_diffs: IndexMap::new(),
        declared_classes: IndexMap::new(),
        deprecated_declared_classes: vec![class_hash0],
        nonces: IndexMap::new(),
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), state_diff1)
        .unwrap()
        .commit()
        .unwrap();

    let state1 = StateNumber(BlockNumber(1));
    let current_class_hash = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_class_hash_at(state1, &contract_address)
        .unwrap()
        .unwrap();

    assert_eq!(current_class_hash, class_hash0);

    let class_hash1 = class_hash!("0x1");
    let state_diff2 = ThinStateDiff {
        deployed_contracts: indexmap! {
            contract_address => class_hash1,
        },
        storage_diffs: IndexMap::new(),
        declared_classes: indexmap! {
            class_hash1 => compiled_class_hash!(4_u8),
        },
        deprecated_declared_classes: Vec::new(),
        nonces: IndexMap::new(),
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(1), state_diff2)
        .unwrap()
        .commit()
        .unwrap();

    // Verify that fetching the class hash returns the new class.
    let state2 = StateNumber(BlockNumber(2));
    let current_class_hash = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_class_hash_at(state2, &contract_address)
        .unwrap()
        .unwrap();

    assert_eq!(current_class_hash, class_hash1);

    // Verify that fetching the class hash from an old state returns the old class.
    let current_class_hash = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_class_hash_at(state1, &contract_address)
        .unwrap()
        .unwrap();

    assert_eq!(current_class_hash, class_hash0);
}

// TODO(shahak): Add test where the state was reverted before the class definitions were written.
#[test]
fn declare_revert_declare_scenario() {
    // Declare a class and a deprecated class.
    let contract_address: ContractAddress = contract_address!("0x11");
    let deprecated_class_hash = class_hash!("0xc1a55");
    let class_hash = class_hash!("0xdec1a55");
    let deprecated_class = DeprecatedContractClass::default();
    let class = SierraContractClass::default();
    let compiled_class_hash = compiled_class_hash!(5_u8);
    let diff0 = ThinStateDiff {
        deployed_contracts: IndexMap::from([(contract_address, deprecated_class_hash)]),
        storage_diffs: IndexMap::new(),
        deprecated_declared_classes: vec![deprecated_class_hash],
        declared_classes: IndexMap::from([(class_hash, compiled_class_hash)]),
        nonces: IndexMap::from([(contract_address, Nonce(StarkHash::from(1_u8)))]),
    };

    let ((reader, mut writer), _temp_dir) = get_test_storage();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0.clone())
        .unwrap()
        .append_classes(
            BlockNumber(0),
            &[(class_hash, &class)],
            &[(deprecated_class_hash, &deprecated_class)],
        )
        .unwrap()
        .commit()
        .unwrap();

    // Assert that both classes are declared.
    let state_number = StateNumber::unchecked_right_after_block(BlockNumber(0));
    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    assert!(state_reader.get_class_definition_at(state_number, &class_hash).unwrap().is_some());
    assert!(
        state_reader
            .get_deprecated_class_definition_at(state_number, &deprecated_class_hash)
            .unwrap()
            .is_some()
    );

    // Revert the block and assert that the classes are no longer declared.
    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    txn.commit().unwrap();
    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    assert!(state_reader.get_class_definition_at(state_number, &class_hash).unwrap().is_none());
    assert!(
        state_reader
            .get_deprecated_class_definition_at(state_number, &deprecated_class_hash)
            .unwrap()
            .is_none()
    );

    // Re-declaring reverted classes should be possible.
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0.clone())
        .unwrap()
        .append_classes(
            BlockNumber(0),
            &[(class_hash, &class)],
            &[(deprecated_class_hash, &deprecated_class)],
        )
        .unwrap()
        .commit()
        .unwrap();

    // Assert that both classes are declared.
    let state_number = StateNumber::unchecked_right_after_block(BlockNumber(0));
    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    assert!(state_reader.get_class_definition_at(state_number, &class_hash).unwrap().is_some());
    assert!(
        state_reader
            .get_deprecated_class_definition_at(state_number, &deprecated_class_hash)
            .unwrap()
            .is_some()
    );
}

/// Tests reverting state diffs when the classes are not written to the storage
#[test]
fn declare_revert_declare_without_classes_scenario() {
    // Declare a class and a deprecated class.
    let contract_address: ContractAddress = contract_address!("0x11");
    let deprecated_class_hash = class_hash!("0xc1a55");
    let class_hash = class_hash!("0xdec1a55");
    let compiled_class_hash = compiled_class_hash!(6_u8);
    let diff0 = ThinStateDiff {
        deployed_contracts: IndexMap::from([(contract_address, deprecated_class_hash)]),
        storage_diffs: IndexMap::new(),
        deprecated_declared_classes: vec![deprecated_class_hash],
        declared_classes: IndexMap::from([(class_hash, compiled_class_hash)]),
        nonces: IndexMap::from([(contract_address, Nonce(StarkHash::from(1_u8)))]),
    };

    let ((reader, mut writer), _temp_dir) = get_test_storage();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Assert that both classes are declared.
    let state_number = StateNumber::unchecked_right_after_block(BlockNumber(0));
    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    assert!(state_reader.get_class_definition_at(state_number, &class_hash).unwrap().is_none());
    assert_eq!(
        state_reader.get_class_definition_block_number(&class_hash).unwrap(),
        Some(BlockNumber(0))
    );
    assert!(
        state_reader
            .get_deprecated_class_definition_at(state_number, &deprecated_class_hash)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        state_reader.get_deprecated_class_definition_block_number(&deprecated_class_hash).unwrap(),
        Some(BlockNumber(0))
    );

    // Revert the block and assert that the classes are no longer declared.
    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    txn.commit().unwrap();
    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    assert!(state_reader.get_class_definition_block_number(&class_hash).unwrap().is_none());
    assert!(
        state_reader
            .get_deprecated_class_definition_block_number(&deprecated_class_hash)
            .unwrap()
            .is_none()
    );

    // Re-declaring reverted classes should be possible.
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Assert that both classes are declared.
    let state_number = StateNumber::unchecked_right_after_block(BlockNumber(0));
    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    assert!(state_reader.get_class_definition_at(state_number, &class_hash).unwrap().is_none());
    assert_eq!(
        state_reader.get_class_definition_block_number(&class_hash).unwrap(),
        Some(BlockNumber(0))
    );
    assert!(
        state_reader
            .get_deprecated_class_definition_at(state_number, &deprecated_class_hash)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        state_reader.get_deprecated_class_definition_block_number(&deprecated_class_hash).unwrap(),
        Some(BlockNumber(0))
    );
}
