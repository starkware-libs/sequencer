use apollo_test_utils::get_test_state_diff;
use assert_matches::assert_matches;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::{indexmap, IndexMap};
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::StarkHash;
use starknet_api::state::{SierraContractClass, StateNumber, ThinStateDiff};
use starknet_api::{class_hash, compiled_class_hash, contract_address, felt, storage_key};
use starknet_types_core::felt::Felt;

use crate::class::{ClassStorageReader, ClassStorageWriter};
use crate::compiled_class::{CasmStorageReader, CasmStorageWriter};
use crate::db::serialization::{ChangesetValueWrapper, NoVersionValueWrapper, ValueSerde};
use crate::db::table_types::{DbCursorTrait, Table};
use crate::state::{StateStorageReader, StateStorageWriter};
use crate::test_utils::{
    get_test_config,
    get_test_storage,
    get_test_storage_with_config_flat_state,
    get_test_storage_with_flat_state,
};
use crate::{open_storage, MarkerKind, StorageError, StorageScope, StorageWriter};

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
        class_hash_to_compiled_class_hash: IndexMap::from([(nc0, compiled_class_hash)]),
        ..Default::default()
    };
    let diff1 = ThinStateDiff {
        deprecated_declared_classes: vec![dc0],
        class_hash_to_compiled_class_hash: IndexMap::from([(nc1, compiled_class_hash)]),
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
        class_hash_to_compiled_class_hash: IndexMap::from([(hash_1, compiled_class_hash)]),
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
        class_hash_to_compiled_class_hash: IndexMap::from([(hash_2, compiled_class_hash)]),
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
        class_hash_to_compiled_class_hash: IndexMap::from([(cl0, c_cls_0), (cl1, c_cls_1)]),
        nonces: IndexMap::from([(c0, Nonce(StarkHash::from(1_u8)))]),
    };
    let diff1 = ThinStateDiff {
        deployed_contracts: IndexMap::from([(c2, cl0), (c0, cl1)]),
        storage_diffs: IndexMap::from([
            (c0, IndexMap::from([(key0, felt!("0x300")), (key1, felt!("0x0"))])),
            (c1, IndexMap::from([(key0, felt!("0x0"))])),
        ]),
        deprecated_declared_classes: vec![cl0],
        class_hash_to_compiled_class_hash: indexmap! {},
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
                class_hash_to_compiled_class_hash: indexmap! { CLASS_HASH => compiled_class_hash!(3_u8) },
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
        class_hash_to_compiled_class_hash: indexmap! {},
        nonces: IndexMap::from([(c0, Nonce(StarkHash::from(1_u8)))]),
    };

    let c1 = contract_address!("0x12");
    let diff1 = ThinStateDiff {
        deployed_contracts: IndexMap::from([(c1, cl0)]),
        storage_diffs: IndexMap::new(),
        deprecated_declared_classes: vec![cl0],
        class_hash_to_compiled_class_hash: indexmap! {},
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
        class_hash_to_compiled_class_hash: IndexMap::from([(class2, compiled_class_hash_2)]),
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
            class_hash_to_compiled_class_hash: IndexMap::new(),
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
        class_hash_to_compiled_class_hash: IndexMap::new(),
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
        class_hash_to_compiled_class_hash: indexmap! {
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
        class_hash_to_compiled_class_hash: IndexMap::from([(class_hash, compiled_class_hash)]),
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
        class_hash_to_compiled_class_hash: IndexMap::from([(class_hash, compiled_class_hash)]),
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

#[test]
fn flat_state_incompatible_with_full_archive() {
    let (mut config, _temp_dir) = get_test_config(Some(StorageScope::FullArchive));
    config.flat_state = true;
    match open_storage(config) {
        Err(StorageError::FlatStateIncompatibleWithFullArchive) => {}
        Err(other) => panic!("Expected FlatStateIncompatibleWithFullArchive, got: {other}"),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn flat_state_write_roundtrip() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0xABC");
    let key = storage_key!("0x01");
    let value = felt!("0x42");
    let nonce = Nonce(felt!("0x01"));
    let class_hash = class_hash!("0x100");
    let compiled = compiled_class_hash!(0x200_u16);

    let diff = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash },
        storage_diffs: indexmap! { address => indexmap! { key => value } },
        nonces: indexmap! { address => nonce },
        class_hash_to_compiled_class_hash: indexmap! { class_hash => compiled },
        ..Default::default()
    };

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff)
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let flat_deployed = txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
    let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();
    let flat_storage = txn.open_table(&txn.tables.flat_contract_storage).unwrap();
    let flat_compiled = txn.open_table(&txn.tables.flat_compiled_class_hash).unwrap();

    assert_eq!(flat_deployed.get(&txn.txn, &address).unwrap(), Some(class_hash));
    assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), Some(nonce));
    assert_eq!(flat_storage.get(&txn.txn, &(address, key)).unwrap(), Some(value));
    assert_eq!(flat_compiled.get(&txn.txn, &class_hash).unwrap(), Some(compiled));
}

#[test]
fn flat_state_disabled_leaves_flat_tables_empty() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    let address = contract_address!("0xABC");
    let class_hash = class_hash!("0x100");
    let diff = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash },
        ..Default::default()
    };

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff)
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let flat_deployed = txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
    assert_eq!(flat_deployed.get(&txn.txn, &address).unwrap(), None);
}

#[test]
fn flat_state_deployment_without_nonce_diff() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0xABC");
    let class_hash = class_hash!("0x100");

    let diff = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff)
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();
    let flat_deployed = txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();

    assert_eq!(flat_deployed.get(&txn.txn, &address).unwrap(), Some(class_hash));
    assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), Some(Nonce::default()));
}

#[test]
fn flat_state_overwrites_on_second_block() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0xABC");
    let key = storage_key!("0x01");

    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash!("0x100") },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x01") } },
        nonces: indexmap! { address => Nonce(felt!("0x01")) },
        ..Default::default()
    };
    let diff1 = ThinStateDiff {
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x02") } },
        nonces: indexmap! { address => Nonce(felt!("0x02")) },
        ..Default::default()
    };

    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff0).unwrap();
    txn = txn.append_state_diff(BlockNumber(1), diff1).unwrap();
    txn.commit().unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let flat_storage = txn.open_table(&txn.tables.flat_contract_storage).unwrap();
    let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();

    assert_eq!(flat_storage.get(&txn.txn, &(address, key)).unwrap(), Some(felt!("0x02")));
    assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), Some(Nonce(felt!("0x02"))));
}

#[test]
fn flat_state_latest_reads_use_flat_path() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0xABC");
    let key = storage_key!("0x01");
    let value = felt!("0x42");
    let nonce = Nonce(felt!("0x01"));
    let class_hash = class_hash!("0x100");
    let compiled = compiled_class_hash!(0x200_u16);

    let diff = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash },
        storage_diffs: indexmap! { address => indexmap! { key => value } },
        nonces: indexmap! { address => nonce },
        class_hash_to_compiled_class_hash: indexmap! { class_hash => compiled },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff)
        .unwrap()
        .commit()
        .unwrap();

    // Overwrite flat tables with sentinel values.
    let sentinel_value = felt!("0xDEAD");
    let sentinel_nonce = Nonce(felt!("0xBEEF"));
    let sentinel_class = class_hash!("0xCAFE");
    let sentinel_compiled = compiled_class_hash!(0xFACE_u16);
    {
        let txn = writer.begin_rw_txn().unwrap();
        let flat_storage = txn.open_table(&txn.tables.flat_contract_storage).unwrap();
        let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();
        let flat_deployed = txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
        let flat_compiled = txn.open_table(&txn.tables.flat_compiled_class_hash).unwrap();
        flat_storage.upsert(&txn.txn, &(address, key), &sentinel_value).unwrap();
        flat_nonces.upsert(&txn.txn, &address, &sentinel_nonce).unwrap();
        flat_deployed.upsert(&txn.txn, &address, &sentinel_class).unwrap();
        flat_compiled.upsert(&txn.txn, &class_hash, &sentinel_compiled).unwrap();
        txn.commit().unwrap();
    }

    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let latest = StateNumber::right_after_block(BlockNumber(0)).unwrap();

    assert_eq!(state_reader.get_storage_at(latest, &address, &key).unwrap(), sentinel_value);
    assert_eq!(state_reader.get_nonce_at(latest, &address).unwrap(), Some(sentinel_nonce));
    assert_eq!(state_reader.get_class_hash_at(latest, &address).unwrap(), Some(sentinel_class));
    assert_eq!(
        state_reader.get_compiled_class_hash_at(latest, &class_hash).unwrap(),
        Some(sentinel_compiled)
    );
}

#[test]
fn flat_state_historical_reads_return_error() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0xABC");
    let key = storage_key!("0x01");

    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash!("0x100") },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x01") } },
        nonces: indexmap! { address => Nonce(felt!("0x01")) },
        ..Default::default()
    };
    let diff1 = ThinStateDiff {
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x02") } },
        nonces: indexmap! { address => Nonce(felt!("0x02")) },
        ..Default::default()
    };

    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff0).unwrap();
    txn = txn.append_state_diff(BlockNumber(1), diff1).unwrap();
    txn.commit().unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let before_block1 = StateNumber::right_before_block(BlockNumber(1));

    // Historical reads on flat_state node should return an error.
    assert_matches!(
        state_reader.get_storage_at(before_block1, &address, &key),
        Err(StorageError::DBInconsistency { .. })
    );
    assert_matches!(
        state_reader.get_nonce_at(before_block1, &address),
        Err(StorageError::DBInconsistency { .. })
    );
}

#[test]
fn flat_state_missing_flat_entries_return_none_or_default() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0xABC");
    let key = storage_key!("0x01");
    let class_hash = class_hash!("0x100");

    // Write an empty diff to advance the state marker.
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), ThinStateDiff::default())
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let latest = StateNumber::right_after_block(BlockNumber(0)).unwrap();

    // With flat_state, missing entries return None/default (no versioned fallback).
    assert_eq!(state_reader.get_storage_at(latest, &address, &key).unwrap(), Felt::default());
    assert_eq!(state_reader.get_nonce_at(latest, &address).unwrap(), None);
    assert_eq!(state_reader.get_class_hash_at(latest, &address).unwrap(), None);
    assert_eq!(state_reader.get_compiled_class_hash_at(latest, &class_hash).unwrap(), None);
}

#[test]
fn flat_state_missing_keys_return_defaults() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let diff = ThinStateDiff::default();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff)
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let latest = StateNumber::right_after_block(BlockNumber(0)).unwrap();

    let missing_addr = contract_address!("0xDEAD");
    let missing_key = storage_key!("0x99");

    assert_eq!(
        state_reader.get_storage_at(latest, &missing_addr, &missing_key).unwrap(),
        Felt::default()
    );
    assert_eq!(state_reader.get_nonce_at(latest, &missing_addr).unwrap(), None);
    assert_eq!(state_reader.get_class_hash_at(latest, &missing_addr).unwrap(), None);
}

#[test]
fn flat_state_revert_restores_previous_values() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0xABC");
    let key = storage_key!("0x01");

    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash!("0x100") },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x01") } },
        nonces: indexmap! { address => Nonce(felt!("0x01")) },
        ..Default::default()
    };
    let diff1 = ThinStateDiff {
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x02") } },
        nonces: indexmap! { address => Nonce(felt!("0x02")) },
        ..Default::default()
    };

    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff0).unwrap();
    txn = txn.append_state_diff(BlockNumber(1), diff1).unwrap();
    txn.commit().unwrap();

    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap();
    txn.commit().unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let flat_storage = txn.open_table(&txn.tables.flat_contract_storage).unwrap();
    let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();

    assert_eq!(flat_storage.get(&txn.txn, &(address, key)).unwrap(), Some(felt!("0x01")));
    assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), Some(Nonce(felt!("0x01"))));
}

#[test]
fn flat_state_revert_first_block_deletes_entries() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0xABC");
    let key = storage_key!("0x01");
    let class_hash = class_hash!("0x100");
    let compiled = compiled_class_hash!(0x200_u16);

    let diff = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x42") } },
        nonces: indexmap! { address => Nonce(felt!("0x01")) },
        class_hash_to_compiled_class_hash: indexmap! { class_hash => compiled },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff)
        .unwrap()
        .commit()
        .unwrap();

    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    txn.commit().unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let flat_deployed = txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
    let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();
    let flat_storage = txn.open_table(&txn.tables.flat_contract_storage).unwrap();
    let flat_compiled = txn.open_table(&txn.tables.flat_compiled_class_hash).unwrap();

    assert_eq!(flat_deployed.get(&txn.txn, &address).unwrap(), None);
    assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), None);
    assert_eq!(flat_storage.get(&txn.txn, &(address, key)).unwrap(), None);
    assert_eq!(flat_compiled.get(&txn.txn, &class_hash).unwrap(), None);
}

#[test]
fn flat_state_revert_deployment_without_nonce_diff() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0xABC");
    let class_hash = class_hash!("0x100");

    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .commit()
        .unwrap();

    {
        let txn = reader.begin_ro_txn().unwrap();
        let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();
        assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), Some(Nonce::default()));
    }

    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    txn.commit().unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let flat_deployed = txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
    let flat_nonces = txn.open_table(&txn.tables.flat_nonces).unwrap();

    assert_eq!(flat_deployed.get(&txn.txn, &address).unwrap(), None);
    assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), None);
}

#[test]
fn flat_state_toggle_off_detected() {
    let ((_, mut writer), config, _temp_dir) = get_test_storage_with_config_flat_state();

    let diff = ThinStateDiff::default();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff)
        .unwrap()
        .commit()
        .unwrap();
    drop(writer);

    let mut config_off = config;
    config_off.flat_state = false;
    let result = open_storage(config_off);
    assert!(matches!(result, Err(StorageError::FlatStateToggleNotSupported)));
}

#[test]
fn changeset_value_wrapper_none_roundtrip() {
    let serialized =
        ChangesetValueWrapper::<NoVersionValueWrapper<Nonce>>::serialize(&None).unwrap();
    assert_eq!(serialized, vec![0x00]);
    let deserialized = ChangesetValueWrapper::<NoVersionValueWrapper<Nonce>>::deserialize(
        &mut serialized.as_slice(),
    )
    .unwrap();
    assert_eq!(deserialized, None);
}

#[test]
fn changeset_value_wrapper_some_default_roundtrip() {
    let wrapper_value = Some(Nonce::default());
    let serialized =
        ChangesetValueWrapper::<NoVersionValueWrapper<Nonce>>::serialize(&wrapper_value).unwrap();
    assert_eq!(serialized[0], 0x01);
    let deserialized = ChangesetValueWrapper::<NoVersionValueWrapper<Nonce>>::deserialize(
        &mut serialized.as_slice(),
    )
    .unwrap();
    assert_eq!(deserialized, Some(Nonce::default()));
    // Crucially: Some(default) != None.
    assert_ne!(deserialized, None);
}

#[test]
fn changeset_value_wrapper_some_nondefault_roundtrip() {
    let nonce = Nonce(felt!("0x42"));
    let serialized =
        ChangesetValueWrapper::<NoVersionValueWrapper<Nonce>>::serialize(&Some(nonce)).unwrap();
    assert_eq!(serialized[0], 0x01);
    let deserialized = ChangesetValueWrapper::<NoVersionValueWrapper<Nonce>>::deserialize(
        &mut serialized.as_slice(),
    )
    .unwrap();
    assert_eq!(deserialized, Some(nonce));
}

#[test]
fn flat_state_requires_fresh_sync_when_state_ahead_of_changeset() {
    let ((reader, mut writer), config, _temp_dir) = get_test_storage_with_config_flat_state();

    // Manually advance state marker without advancing changeset marker.
    {
        let txn = writer.begin_rw_txn().unwrap();
        let markers_table = txn.open_table(&txn.tables.markers).unwrap();
        markers_table.upsert(&txn.txn, &MarkerKind::State, &BlockNumber(1)).unwrap();
        txn.commit().unwrap();
    }

    // Re-open should fail.
    drop(reader);
    drop(writer);

    let result = open_storage(config);
    let err = result.err().expect("Expected FlatStateRequiresFreshSync error");
    assert!(
        err.to_string().contains("fresh sync"),
        "Expected FlatStateRequiresFreshSync error, got: {err}"
    );
}

#[test]
fn flat_state_fresh_sync_passes_when_markers_match() {
    let ((_reader, _writer), config, _temp_dir) = get_test_storage_with_config_flat_state();
    // Fresh node: both markers are 0. Re-open should succeed.
    drop(_reader);
    drop(_writer);
    let result = open_storage(config);
    assert!(result.is_ok());
}

#[test]
fn changeset_write_captures_preimages() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");
    let class_hash_0 = class_hash!("0xaa");
    // Block 0: first writes (no prior values).
    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash_0 },
        nonces: indexmap! { address => Nonce(felt!("0x1")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x100") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => CompiledClassHash(felt!("0xcc"))
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .commit()
        .unwrap();

    // Block 1: update all values.
    let class_hash_1 = class_hash!("0xbb");
    let diff1 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash_1 },
        nonces: indexmap! { address => Nonce(felt!("0x2")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x200") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => CompiledClassHash(felt!("0xee"))
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(1), diff1)
        .unwrap()
        .commit()
        .unwrap();

    // Verify block 0 changesets: all None (no prior values existed).
    let txn = reader.begin_ro_txn().unwrap();
    let changeset_deployed = txn.txn.open_table(&txn.tables.changeset_deployed_contracts).unwrap();
    let changeset_nonces = txn.txn.open_table(&txn.tables.changeset_nonces).unwrap();
    let changeset_storage = txn.txn.open_table(&txn.tables.changeset_contract_storage).unwrap();
    let changeset_compiled = txn.txn.open_table(&txn.tables.changeset_compiled_class_hash).unwrap();

    assert_eq!(
        changeset_deployed.get(&txn.txn, &(BlockNumber(0), address)).unwrap(),
        Some(None),
        "Block 0 deployed changeset should be None (no prior value)"
    );
    assert_eq!(
        changeset_nonces.get(&txn.txn, &(BlockNumber(0), address)).unwrap(),
        Some(None),
        "Block 0 nonce changeset should be None (no prior value)"
    );
    assert_eq!(
        changeset_storage.get(&txn.txn, &((BlockNumber(0), address), key)).unwrap(),
        Some(None),
        "Block 0 storage changeset should be None (no prior value)"
    );
    assert_eq!(
        changeset_compiled.get(&txn.txn, &(BlockNumber(0), ClassHash(felt!("0xdd")))).unwrap(),
        Some(None),
        "Block 0 compiled changeset should be None (no prior value)"
    );

    // Verify block 1 changesets: have block 0 values.
    assert_eq!(
        changeset_deployed.get(&txn.txn, &(BlockNumber(1), address)).unwrap(),
        Some(Some(class_hash_0)),
        "Block 1 deployed changeset should have block 0 class hash"
    );
    // Nonce pre-image for block 1 should be block 0's final nonce (0x1).
    assert_eq!(
        changeset_nonces.get(&txn.txn, &(BlockNumber(1), address)).unwrap(),
        Some(Some(Nonce(felt!("0x1")))),
        "Block 1 nonce changeset should have block 0 nonce"
    );
    assert_eq!(
        changeset_storage.get(&txn.txn, &((BlockNumber(1), address), key)).unwrap(),
        Some(Some(felt!("0x100"))),
        "Block 1 storage changeset should have block 0 value"
    );
    assert_eq!(
        changeset_compiled.get(&txn.txn, &(BlockNumber(1), ClassHash(felt!("0xdd")))).unwrap(),
        Some(Some(CompiledClassHash(felt!("0xcc")))),
        "Block 1 compiled changeset should have block 0 compiled hash"
    );
}

#[test]
fn changeset_distinguishes_none_from_default_nonce() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");

    // Block 0: deploy contract (implicit nonce=0).
    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash!("0xaa") },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .commit()
        .unwrap();

    // Block 1: change nonce.
    let diff1 = ThinStateDiff {
        nonces: indexmap! { address => Nonce(felt!("0x5")) },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(1), diff1)
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let changeset_nonces = txn.txn.open_table(&txn.tables.changeset_nonces).unwrap();

    // Block 0: nonce changeset for the deployed contract should be None (no prior nonce).
    assert_eq!(
        changeset_nonces.get(&txn.txn, &(BlockNumber(0), address)).unwrap(),
        Some(None),
        "Block 0 changeset nonce should be None (contract didn't exist before)"
    );

    // Block 1: nonce changeset should be Some(Nonce::default()) since block 0 set nonce to 0.
    assert_eq!(
        changeset_nonces.get(&txn.txn, &(BlockNumber(1), address)).unwrap(),
        Some(Some(Nonce::default())),
        "Block 1 changeset nonce should be Some(Nonce(0)), not None"
    );
}

#[test]
fn changeset_marker_tracks_progress() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    for block in 0..3 {
        writer
            .begin_rw_txn()
            .unwrap()
            .append_state_diff(BlockNumber(block), ThinStateDiff::default())
            .unwrap()
            .commit()
            .unwrap();
    }

    let txn = reader.begin_ro_txn().unwrap();
    let changeset_marker = txn.get_changeset_marker().unwrap();
    assert_eq!(
        changeset_marker,
        BlockNumber(3),
        "Changeset marker should be 3 after writing 3 blocks"
    );
}

#[test]
fn empty_state_diff_advances_changeset_marker_without_rows() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), ThinStateDiff::default())
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();

    // Marker should have advanced.
    let changeset_marker = txn.get_changeset_marker().unwrap();
    assert_eq!(changeset_marker, BlockNumber(1));

    // All changeset tables should be empty.
    let changeset_deployed = txn.txn.open_table(&txn.tables.changeset_deployed_contracts).unwrap();
    let changeset_nonces = txn.txn.open_table(&txn.tables.changeset_nonces).unwrap();
    let changeset_storage = txn.txn.open_table(&txn.tables.changeset_contract_storage).unwrap();
    let changeset_compiled = txn.txn.open_table(&txn.tables.changeset_compiled_class_hash).unwrap();

    let mut cursor = changeset_deployed.cursor(&txn.txn).unwrap();
    assert!(cursor.next().unwrap().is_none(), "changeset_deployed should be empty");
    let mut cursor = changeset_nonces.cursor(&txn.txn).unwrap();
    assert!(cursor.next().unwrap().is_none(), "changeset_nonces should be empty");
    let mut cursor = changeset_storage.cursor(&txn.txn).unwrap();
    assert!(cursor.next().unwrap().is_none(), "changeset_storage should be empty");
    let mut cursor = changeset_compiled.cursor(&txn.txn).unwrap();
    assert!(cursor.next().unwrap().is_none(), "changeset_compiled should be empty");
}

#[test]
fn changeset_revert_restores_flat_state() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");
    let class_hash_0 = class_hash!("0xaa");
    let compiled_hash_0 = CompiledClassHash(felt!("0xcc"));

    // Block 0: deploy contract with storage and compiled class hash.
    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash_0 },
        nonces: indexmap! { address => Nonce(felt!("0x1")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x100") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => compiled_hash_0
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .commit()
        .unwrap();

    // Block 1: update all values.
    let class_hash_1 = class_hash!("0xbb");
    let diff1 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash_1 },
        nonces: indexmap! { address => Nonce(felt!("0x2")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x200") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => CompiledClassHash(felt!("0xee"))
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(1), diff1)
        .unwrap()
        .commit()
        .unwrap();

    // Revert block 1.
    writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap().0.commit().unwrap();

    // Verify flat tables have block 0 values.
    let txn = reader.begin_ro_txn().unwrap();
    let flat_deployed = txn.txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
    let flat_nonces = txn.txn.open_table(&txn.tables.flat_nonces).unwrap();
    let flat_storage = txn.txn.open_table(&txn.tables.flat_contract_storage).unwrap();
    let flat_compiled = txn.txn.open_table(&txn.tables.flat_compiled_class_hash).unwrap();

    assert_eq!(
        flat_deployed.get(&txn.txn, &address).unwrap(),
        Some(class_hash_0),
        "Flat deployed should have block 0 class hash after revert"
    );
    assert_eq!(
        flat_nonces.get(&txn.txn, &address).unwrap(),
        Some(Nonce(felt!("0x1"))),
        "Flat nonce should have block 0 nonce after revert"
    );
    assert_eq!(
        flat_storage.get(&txn.txn, &(address, key)).unwrap(),
        Some(felt!("0x100")),
        "Flat storage should have block 0 value after revert"
    );
    assert_eq!(
        flat_compiled.get(&txn.txn, &ClassHash(felt!("0xdd"))).unwrap(),
        Some(compiled_hash_0),
        "Flat compiled should have block 0 compiled hash after revert"
    );

    // Verify changeset entries for block 1 are deleted.
    let changeset_deployed = txn.txn.open_table(&txn.tables.changeset_deployed_contracts).unwrap();
    let changeset_nonces = txn.txn.open_table(&txn.tables.changeset_nonces).unwrap();
    let changeset_storage = txn.txn.open_table(&txn.tables.changeset_contract_storage).unwrap();
    let changeset_compiled = txn.txn.open_table(&txn.tables.changeset_compiled_class_hash).unwrap();

    assert_eq!(
        changeset_deployed.get(&txn.txn, &(BlockNumber(1), address)).unwrap(),
        None,
        "Block 1 changeset_deployed should be deleted after revert"
    );
    assert_eq!(
        changeset_nonces.get(&txn.txn, &(BlockNumber(1), address)).unwrap(),
        None,
        "Block 1 changeset_nonces should be deleted after revert"
    );
    assert_eq!(
        changeset_storage.get(&txn.txn, &((BlockNumber(1), address), key)).unwrap(),
        None,
        "Block 1 changeset_storage should be deleted after revert"
    );
    assert_eq!(
        changeset_compiled.get(&txn.txn, &(BlockNumber(1), ClassHash(felt!("0xdd")))).unwrap(),
        None,
        "Block 1 changeset_compiled should be deleted after revert"
    );

    // Block 0 changeset entries should still exist.
    assert!(
        changeset_deployed.get(&txn.txn, &(BlockNumber(0), address)).unwrap().is_some(),
        "Block 0 changeset_deployed should still exist"
    );

    // Verify changeset marker decremented.
    let changeset_marker = txn.get_changeset_marker().unwrap();
    assert_eq!(
        changeset_marker,
        BlockNumber(1),
        "Changeset marker should be 1 after reverting block 1"
    );
}

#[test]
fn changeset_revert_first_block_deletes_flat_entries() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");

    // Block 0: deploy, set nonce, storage, and compiled class hash.
    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash!("0xaa") },
        nonces: indexmap! { address => Nonce(felt!("0x1")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x100") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => CompiledClassHash(felt!("0xcc"))
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .commit()
        .unwrap();

    // Revert block 0.
    writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap().0.commit().unwrap();

    // Verify all flat tables are empty.
    let txn = reader.begin_ro_txn().unwrap();
    let flat_deployed = txn.txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
    let flat_nonces = txn.txn.open_table(&txn.tables.flat_nonces).unwrap();
    let flat_storage = txn.txn.open_table(&txn.tables.flat_contract_storage).unwrap();
    let flat_compiled = txn.txn.open_table(&txn.tables.flat_compiled_class_hash).unwrap();

    assert_eq!(
        flat_deployed.get(&txn.txn, &address).unwrap(),
        None,
        "Flat deployed should be empty after reverting first block"
    );
    assert_eq!(
        flat_nonces.get(&txn.txn, &address).unwrap(),
        None,
        "Flat nonces should be empty after reverting first block"
    );
    assert_eq!(
        flat_storage.get(&txn.txn, &(address, key)).unwrap(),
        None,
        "Flat storage should be empty after reverting first block"
    );
    assert_eq!(
        flat_compiled.get(&txn.txn, &ClassHash(felt!("0xdd"))).unwrap(),
        None,
        "Flat compiled should be empty after reverting first block"
    );

    // Verify changeset marker is at 0.
    let changeset_marker = txn.get_changeset_marker().unwrap();
    assert_eq!(
        changeset_marker,
        BlockNumber(0),
        "Changeset marker should be 0 after reverting first block"
    );
}

#[test]
fn changeset_revert_preserves_default_nonce() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");

    // Block 0: deploy contract (implicit nonce=0).
    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash!("0xaa") },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .commit()
        .unwrap();

    // Block 1: change nonce.
    let diff1 = ThinStateDiff {
        nonces: indexmap! { address => Nonce(felt!("0x5")) },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(1), diff1)
        .unwrap()
        .commit()
        .unwrap();

    // Revert block 1.
    writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap().0.commit().unwrap();

    // Flat nonce should be Some(Nonce::default()), NOT deleted.
    let txn = reader.begin_ro_txn().unwrap();
    let flat_nonces = txn.txn.open_table(&txn.tables.flat_nonces).unwrap();
    assert_eq!(
        flat_nonces.get(&txn.txn, &address).unwrap(),
        Some(Nonce::default()),
        "Flat nonce should be Some(Nonce(0)) after revert, not deleted"
    );

    // Verify changeset entries for block 1 are cleaned up.
    let changeset_nonces = txn.txn.open_table(&txn.tables.changeset_nonces).unwrap();
    assert_eq!(
        changeset_nonces.get(&txn.txn, &(BlockNumber(1), address)).unwrap(),
        None,
        "Block 1 changeset nonce should be deleted after revert"
    );

    // Verify changeset marker decremented.
    let changeset_marker = txn.get_changeset_marker().unwrap();
    assert_eq!(
        changeset_marker,
        BlockNumber(1),
        "Changeset marker should be 1 after reverting block 1"
    );
}

#[test]
fn get_reversed_state_diff_from_changeset_matches_versioned() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");
    let class_hash_0 = class_hash!("0xaa");

    // Block 0: deploy contract, set nonce and storage.
    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash_0 },
        nonces: indexmap! { address => Nonce(felt!("0x1")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x100") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => CompiledClassHash(felt!("0xcc"))
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .commit()
        .unwrap();

    // Block 1: update all values (no new deployments, just class replacement).
    let class_hash_1 = class_hash!("0xbb");
    let diff1 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash_1 },
        nonces: indexmap! { address => Nonce(felt!("0x2")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x200") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => CompiledClassHash(felt!("0xee"))
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(1), diff1)
        .unwrap()
        .commit()
        .unwrap();

    // Get reversed state diff for block 1 — should contain block 0 values.
    let txn = reader.begin_ro_txn().unwrap();
    let reversed = txn.get_reversed_state_diff_from_changeset(BlockNumber(1)).unwrap();

    // Deployed contracts: block 1 replaced the class, so pre-image is block 0's class hash.
    assert_eq!(reversed.deployed_contracts.get(&address), Some(&class_hash_0));

    // Nonces: block 1 changed nonce from 0x1 to 0x2, so pre-image is 0x1.
    assert_eq!(reversed.nonces.get(&address), Some(&Nonce(felt!("0x1"))));

    // Storage: block 1 changed value from 0x100 to 0x200, so pre-image is 0x100.
    assert_eq!(
        reversed.storage_diffs.get(&address).and_then(|m| m.get(&key)),
        Some(&felt!("0x100"))
    );

    // Compiled class hash: was 0xcc, now 0xee, so pre-image is 0xcc.
    assert_eq!(
        reversed.class_hash_to_compiled_class_hash.get(&ClassHash(felt!("0xdd"))),
        Some(&CompiledClassHash(felt!("0xcc")))
    );

    // Deprecated declared classes are always empty from changesets.
    assert!(reversed.deprecated_declared_classes.is_empty());
}

#[test]
fn get_reversed_state_diff_from_changeset_block_zero() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");

    // Block 0: deploy contract, set nonce and storage (nothing existed before).
    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash!("0xaa") },
        nonces: indexmap! { address => Nonce(felt!("0x1")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x100") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => CompiledClassHash(felt!("0xcc"))
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .commit()
        .unwrap();

    // Get reversed state diff for block 0 — pre-images should all be defaults (None -> default).
    let txn = reader.begin_ro_txn().unwrap();
    let reversed = txn.get_reversed_state_diff_from_changeset(BlockNumber(0)).unwrap();

    // Deployed contracts: None pre-image becomes ClassHash::default().
    assert_eq!(reversed.deployed_contracts.get(&address), Some(&ClassHash::default()));

    // Nonces: None pre-image becomes Nonce::default().
    assert_eq!(reversed.nonces.get(&address), Some(&Nonce::default()));

    // Storage: None pre-image becomes Felt::default() (zero).
    assert_eq!(
        reversed.storage_diffs.get(&address).and_then(|m| m.get(&key)),
        Some(&Felt::default())
    );

    // Compiled class hash: None pre-image becomes CompiledClassHash::default().
    assert_eq!(
        reversed.class_hash_to_compiled_class_hash.get(&ClassHash(felt!("0xdd"))),
        Some(&CompiledClassHash::default())
    );
}

#[test]
fn get_reversed_state_diff_from_changeset_errors_on_missing_block() {
    let ((reader, _writer), _temp_dir) = get_test_storage_with_flat_state();

    // No blocks written, changeset marker is 0. Requesting block 0 should fail.
    let txn = reader.begin_ro_txn().unwrap();
    let result = txn.get_reversed_state_diff_from_changeset(BlockNumber(0));
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("changeset marker"),
        "Expected error about changeset marker"
    );
}

#[test]
fn versioned_tables_skipped_when_flat_state_enabled() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");
    let class_hash = class_hash!("0xaa");
    let compiled_hash = CompiledClassHash(felt!("0xcc"));

    let diff = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash },
        nonces: indexmap! { address => Nonce(felt!("0x1")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x100") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => compiled_hash
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff)
        .unwrap()
        .commit()
        .unwrap();

    // Verify versioned tables are empty.
    let txn = reader.begin_ro_txn().unwrap();
    let deployed_contracts = txn.txn.open_table(&txn.tables.deployed_contracts).unwrap();
    let nonces = txn.txn.open_table(&txn.tables.nonces).unwrap();
    let storage = txn.txn.open_table(&txn.tables.contract_storage).unwrap();
    let compiled = txn.txn.open_table(&txn.tables.compiled_class_hash).unwrap();

    assert_eq!(
        deployed_contracts.get(&txn.txn, &(address, BlockNumber(0))).unwrap(),
        None,
        "Versioned deployed_contracts should be empty when flat_state is enabled"
    );
    assert_eq!(
        nonces.get(&txn.txn, &(address, BlockNumber(0))).unwrap(),
        None,
        "Versioned nonces should be empty when flat_state is enabled"
    );
    assert_eq!(
        storage.get(&txn.txn, &((address, key), BlockNumber(0))).unwrap(),
        None,
        "Versioned storage should be empty when flat_state is enabled"
    );
    assert_eq!(
        compiled.get(&txn.txn, &(ClassHash(felt!("0xdd")), BlockNumber(0))).unwrap(),
        None,
        "Versioned compiled_class_hash should be empty when flat_state is enabled"
    );

    // Verify flat tables are populated.
    let flat_deployed = txn.txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
    let flat_nonces = txn.txn.open_table(&txn.tables.flat_nonces).unwrap();
    let flat_storage = txn.txn.open_table(&txn.tables.flat_contract_storage).unwrap();
    let flat_compiled = txn.txn.open_table(&txn.tables.flat_compiled_class_hash).unwrap();

    assert_eq!(flat_deployed.get(&txn.txn, &address).unwrap(), Some(class_hash));
    assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), Some(Nonce(felt!("0x1"))));
    assert_eq!(flat_storage.get(&txn.txn, &(address, key)).unwrap(), Some(felt!("0x100")));
    assert_eq!(
        flat_compiled.get(&txn.txn, &ClassHash(felt!("0xdd"))).unwrap(),
        Some(compiled_hash)
    );
}

#[test]
fn versioned_tables_still_written_when_flat_state_disabled() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");
    let class_hash = class_hash!("0xaa");
    let compiled_hash = CompiledClassHash(felt!("0xcc"));

    let diff = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash },
        nonces: indexmap! { address => Nonce(felt!("0x1")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x100") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => compiled_hash
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff)
        .unwrap()
        .commit()
        .unwrap();

    // Verify versioned tables are populated.
    let txn = reader.begin_ro_txn().unwrap();
    let deployed_contracts = txn.txn.open_table(&txn.tables.deployed_contracts).unwrap();
    let nonces = txn.txn.open_table(&txn.tables.nonces).unwrap();
    let storage = txn.txn.open_table(&txn.tables.contract_storage).unwrap();
    let compiled = txn.txn.open_table(&txn.tables.compiled_class_hash).unwrap();

    assert_eq!(
        deployed_contracts.get(&txn.txn, &(address, BlockNumber(0))).unwrap(),
        Some(class_hash),
        "Versioned deployed_contracts should be populated when flat_state is disabled"
    );
    // Nonce: deployed contract gets implicit Nonce::default(), then nonces diff writes 0x1.
    assert_eq!(
        nonces.get(&txn.txn, &(address, BlockNumber(0))).unwrap(),
        Some(Nonce(felt!("0x1"))),
        "Versioned nonces should be populated when flat_state is disabled"
    );
    assert_eq!(
        storage.get(&txn.txn, &((address, key), BlockNumber(0))).unwrap(),
        Some(felt!("0x100")),
        "Versioned storage should be populated when flat_state is disabled"
    );
    assert_eq!(
        compiled.get(&txn.txn, &(ClassHash(felt!("0xdd")), BlockNumber(0))).unwrap(),
        Some(compiled_hash),
        "Versioned compiled_class_hash should be populated when flat_state is disabled"
    );
}

#[test]
fn revert_without_versioned_tables() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");
    let class_hash_0 = class_hash!("0xaa");
    let compiled_hash_0 = CompiledClassHash(felt!("0xcc"));

    // Block 0.
    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash_0 },
        nonces: indexmap! { address => Nonce(felt!("0x1")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x100") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => compiled_hash_0
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0)
        .unwrap()
        .commit()
        .unwrap();

    // Block 1: update all values.
    let diff1 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash!("0xbb") },
        nonces: indexmap! { address => Nonce(felt!("0x2")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x200") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => CompiledClassHash(felt!("0xee"))
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(1), diff1)
        .unwrap()
        .commit()
        .unwrap();

    // Revert block 1.
    writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap().0.commit().unwrap();

    // Verify flat tables restored to block 0 values.
    let txn = reader.begin_ro_txn().unwrap();
    let flat_deployed = txn.txn.open_table(&txn.tables.flat_deployed_contracts).unwrap();
    let flat_nonces = txn.txn.open_table(&txn.tables.flat_nonces).unwrap();
    let flat_storage = txn.txn.open_table(&txn.tables.flat_contract_storage).unwrap();
    let flat_compiled = txn.txn.open_table(&txn.tables.flat_compiled_class_hash).unwrap();

    assert_eq!(flat_deployed.get(&txn.txn, &address).unwrap(), Some(class_hash_0));
    assert_eq!(flat_nonces.get(&txn.txn, &address).unwrap(), Some(Nonce(felt!("0x1"))));
    assert_eq!(flat_storage.get(&txn.txn, &(address, key)).unwrap(), Some(felt!("0x100")));
    assert_eq!(
        flat_compiled.get(&txn.txn, &ClassHash(felt!("0xdd"))).unwrap(),
        Some(compiled_hash_0)
    );

    // State marker should be back to 1.
    assert_eq!(txn.get_state_marker().unwrap(), BlockNumber(1));
}

#[test]
fn flat_only_reads_work_without_versioned_fallback() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");
    let class_hash = class_hash!("0xaa");
    let compiled_hash = CompiledClassHash(felt!("0xcc"));

    let diff = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash },
        nonces: indexmap! { address => Nonce(felt!("0x1")) },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x100") } },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(felt!("0xdd")) => compiled_hash
        },
        ..Default::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff)
        .unwrap()
        .commit()
        .unwrap();

    // Read via state reader (flat path only, no versioned tables populated).
    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let latest = StateNumber::right_after_block(BlockNumber(0)).unwrap();

    assert_eq!(state_reader.get_class_hash_at(latest, &address).unwrap(), Some(class_hash));
    assert_eq!(state_reader.get_nonce_at(latest, &address).unwrap(), Some(Nonce(felt!("0x1"))));
    assert_eq!(state_reader.get_storage_at(latest, &address, &key).unwrap(), felt!("0x100"));
    assert_eq!(
        state_reader.get_compiled_class_hash_at(latest, &ClassHash(felt!("0xdd"))).unwrap(),
        Some(compiled_hash)
    );

    // Verify versioned tables are indeed empty.
    let deployed_contracts = txn.txn.open_table(&txn.tables.deployed_contracts).unwrap();
    assert_eq!(
        deployed_contracts.get(&txn.txn, &(address, BlockNumber(0))).unwrap(),
        None,
        "Versioned tables should be empty"
    );
}

#[test]
fn revert_non_tip_block_returns_none() {
    let ((_, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");

    // Write 2 blocks.
    let diff0 = ThinStateDiff {
        deployed_contracts: indexmap! { address => class_hash!("0xaa") },
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x100") } },
        ..Default::default()
    };
    let diff1 = ThinStateDiff {
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x200") } },
        ..Default::default()
    };
    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff0).unwrap();
    txn = txn.append_state_diff(BlockNumber(1), diff1).unwrap();
    txn.commit().unwrap();

    // Try to revert block 0 (not the tip) — should return None.
    let (_, reverted) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    assert!(reverted.is_none(), "Reverting a non-tip block should return None");
}

#[test]
fn pruning_requires_flat_state() {
    let (mut config, _temp_dir) = get_test_config(Some(StorageScope::StateOnly));
    config.flat_state = false;
    config.changeset_retention_blocks = Some(1000);
    match open_storage(config) {
        Err(StorageError::PruningRequiresFlatState) => {}
        Err(other) => panic!("Expected PruningRequiresFlatState, got: {other}"),
        Ok(_) => panic!("Expected PruningRequiresFlatState error, got Ok"),
    }
}

#[test]
fn get_reversed_state_diff_from_changeset_errors_on_pruned_block() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_flat_state();

    // Write 3 blocks with some state.
    let address = contract_address!("0x1");
    let key = storage_key!("0x10");
    for block in 0u64..3 {
        let diff = ThinStateDiff {
            storage_diffs: indexmap! {
                address => indexmap! {
                    key => Felt::from(block + 1),
                },
            },
            ..Default::default()
        };
        writer
            .begin_rw_txn()
            .unwrap()
            .append_state_diff(BlockNumber(block), diff)
            .unwrap()
            .commit()
            .unwrap();
    }

    // Manually advance the ChangesetPruned marker to simulate pruning of blocks 0 and 1.
    {
        let txn = writer.begin_rw_txn().unwrap();
        let markers_table = txn.open_table(&txn.tables.markers).unwrap();
        markers_table.upsert(&txn.txn, &MarkerKind::ChangesetPruned, &BlockNumber(2)).unwrap();
        txn.commit().unwrap();
    }

    let txn = reader.begin_ro_txn().unwrap();

    // Requesting a pruned block should error.
    let result = txn.get_reversed_state_diff_from_changeset(BlockNumber(0));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("changeset history pruned"));

    let result = txn.get_reversed_state_diff_from_changeset(BlockNumber(1));
    assert!(result.is_err());

    // Requesting a non-pruned block should succeed.
    let result = txn.get_reversed_state_diff_from_changeset(BlockNumber(2));
    assert!(result.is_ok());
}
