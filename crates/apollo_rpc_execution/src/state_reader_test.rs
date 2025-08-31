use std::cell::Cell;
use std::sync::Arc;

use apollo_class_manager_types::{
    ClassManagerClientError,
    ClassManagerError,
    MockClassManagerClient,
    SharedClassManagerClient,
};
use apollo_storage::body::BodyStorageWriter;
use apollo_storage::class::ClassStorageWriter;
use apollo_storage::compiled_class::CasmStorageWriter;
use apollo_storage::header::HeaderStorageWriter;
use apollo_storage::state::StateStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use apollo_storage::StorageWriter;
use assert_matches::assert_matches;
use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::StateReader;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_utils::bigint::BigUintAsHex;
use indexmap::indexmap;
use papyrus_common::pending_classes::{ApiContractClass, PendingClasses, PendingClassesTrait};
use papyrus_common::state::{
    DeclaredClassHashEntry,
    DeployedContract,
    ReplacedClass,
    StorageEntry,
};
use starknet_api::block::{BlockBody, BlockHash, BlockHeader, BlockHeaderWithoutHash, BlockNumber};
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::state::{SierraContractClass, StateNumber, ThinStateDiff};
use starknet_api::{class_hash, compiled_class_hash, contract_address, felt, storage_key};
use starknet_types_core::felt::Felt;

use crate::objects::PendingData;
use crate::state_reader::ExecutionStateReader;
use crate::test_utils::{get_test_casm, get_test_deprecated_contract_class};

const CONTRACT_ADDRESS: &str = "0x2";
const DEPRECATED_CONTRACT_ADDRESS: &str = "0x1";

#[test]
fn read_state() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();

    let class_hash0 = ClassHash(2u128.into());
    let address0 = contract_address!(CONTRACT_ADDRESS);
    let storage_key0 = storage_key!("0x0");
    let storage_value0 = felt!(777_u128);
    let storage_value1 = felt!(888_u128);
    // The class is not used in the execution, so it can be default.
    let class0 = SierraContractClass::default();
    let sierra_version0 = class0.get_sierra_version().unwrap();
    let casm0 = get_test_casm();
    let blockifier_casm0 = RunnableCompiledClass::V1(
        CompiledClassV1::try_from((casm0.clone(), sierra_version0)).unwrap(),
    );
    let compiled_class_hash0 = CompiledClassHash(StarkHash::default());

    let class_hash1 = ClassHash(1u128.into());
    let class1 = get_test_deprecated_contract_class();
    let address1 = contract_address!(DEPRECATED_CONTRACT_ADDRESS);
    let nonce0 = Nonce(felt!(1_u128));

    let address2 = contract_address!("0x123");
    let storage_value2 = felt!(999_u128);
    let class_hash2 = ClassHash(1234u128.into());
    let compiled_class_hash2 = CompiledClassHash(StarkHash::TWO);
    let mut casm2 = get_test_casm();
    casm2.bytecode[0] = BigUintAsHex { value: 12345u32.into() };
    let class2 = SierraContractClass::default();
    let sierra_version2 = class2.get_sierra_version().unwrap();
    let blockifier_casm2 = RunnableCompiledClass::V1(
        CompiledClassV1::try_from((casm2.clone(), sierra_version2)).unwrap(),
    );
    let nonce1 = Nonce(felt!(2_u128));
    let class_hash3 = ClassHash(567_u128.into());
    let class_hash4 = ClassHash(89_u128.into());
    let class_hash5 = ClassHash(98765_u128.into());

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_state_diff(BlockNumber(0), ThinStateDiff::default())
        .unwrap()
        .append_classes(BlockNumber(0), &[], &[])
        .unwrap()
        .append_header(
            BlockNumber(1),
            &BlockHeader {
                block_hash: BlockHash(felt!(1_u128)),
                block_header_without_hash: BlockHeaderWithoutHash {
                    block_number: BlockNumber(1),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(1), BlockBody::default())
        .unwrap()
        .append_state_diff(
            BlockNumber(1),
            ThinStateDiff {
                deployed_contracts: indexmap!(
                    address0 => class_hash0,
                    address1 => class_hash1,
                ),
                storage_diffs: indexmap!(
                    address0 => indexmap!(
                        storage_key0 => storage_value0,
                    ),
                ),
                declared_classes: indexmap!(
                    class_hash0 => compiled_class_hash0,
                    class_hash5 => compiled_class_hash0,
                ),
                deprecated_declared_classes: vec![class_hash1],
                nonces: indexmap!(
                    address0 => nonce0,
                    address1 => Nonce::default(),
                ),
            },
        )
        .unwrap()
        .append_classes(
            BlockNumber(1),
            &[(class_hash0, &class0), (class_hash5, &class0)],
            &[(class_hash1, &class1)],
        )
        .unwrap()
        .append_casm(&class_hash0, &casm0)
        .unwrap()
        .append_header(
            BlockNumber(2),
            &BlockHeader {
                block_hash: BlockHash(felt!(2_u128)),
                block_header_without_hash: BlockHeaderWithoutHash {
                    block_number: BlockNumber(2),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(2), BlockBody::default())
        .unwrap()
        .append_state_diff(BlockNumber(2), ThinStateDiff::default())
        .unwrap()
        .append_classes(BlockNumber(2), &[], &[])
        .unwrap()
        .commit()
        .unwrap();

    let state_number0 = StateNumber::unchecked_right_after_block(BlockNumber(0));
    let state_reader0 = ExecutionStateReader {
        storage_reader: storage_reader.clone(),
        state_number: state_number0,
        maybe_pending_data: None,
        missing_compiled_class: Cell::new(None),
        class_manager_handle: None,
    };
    let storage_after_block_0 = state_reader0.get_storage_at(address0, storage_key0).unwrap();
    assert_eq!(storage_after_block_0, Felt::default());
    let nonce_after_block_0 = state_reader0.get_nonce_at(address0).unwrap();
    assert_eq!(nonce_after_block_0, Nonce::default());
    let class_hash_after_block_0 = state_reader0.get_class_hash_at(address0).unwrap();
    assert_eq!(class_hash_after_block_0, ClassHash::default());
    let compiled_contract_class_after_block_0 = state_reader0.get_compiled_class(class_hash0);
    assert_matches!(
        compiled_contract_class_after_block_0, Err(StateError::UndeclaredClassHash(class_hash))
        if class_hash == class_hash0
    );
    assert_eq!(state_reader0.get_compiled_class_hash(class_hash0).unwrap(), compiled_class_hash0);

    let state_number1 = StateNumber::unchecked_right_after_block(BlockNumber(1));
    let state_reader1 = ExecutionStateReader {
        storage_reader: storage_reader.clone(),
        state_number: state_number1,
        maybe_pending_data: None,
        missing_compiled_class: Cell::new(None),
        class_manager_handle: None,
    };
    let storage_after_block_1 = state_reader1.get_storage_at(address0, storage_key0).unwrap();
    assert_eq!(storage_after_block_1, storage_value0);
    let nonce_after_block_1 = state_reader1.get_nonce_at(address0).unwrap();
    assert_eq!(nonce_after_block_1, nonce0);
    let class_hash_after_block_1 = state_reader1.get_class_hash_at(address0).unwrap();
    assert_eq!(class_hash_after_block_1, class_hash0);
    let compiled_contract_class_after_block_1 =
        state_reader1.get_compiled_class(class_hash0).unwrap();
    assert_eq!(compiled_contract_class_after_block_1, blockifier_casm0);

    // Test that an error is returned if we try to get a missing casm, and the field
    // `missing_compiled_class` is set to the missing casm's hash.
    state_reader1.get_compiled_class(class_hash5).unwrap_err();
    assert_eq!(state_reader1.missing_compiled_class.get().unwrap(), class_hash5);

    let state_number2 = StateNumber::unchecked_right_after_block(BlockNumber(2));
    let mut state_reader2 = ExecutionStateReader {
        storage_reader,
        state_number: state_number2,
        maybe_pending_data: None,
        missing_compiled_class: Cell::new(None),
        class_manager_handle: None,
    };
    let nonce_after_block_2 = state_reader2.get_nonce_at(address0).unwrap();
    assert_eq!(nonce_after_block_2, nonce0);

    // Test pending state diff
    let mut pending_classes = PendingClasses::default();
    pending_classes.add_compiled_class(class_hash2, casm2);
    pending_classes.add_class(class_hash2, ApiContractClass::ContractClass(class2));
    pending_classes.add_class(class_hash3, ApiContractClass::ContractClass(class0));
    pending_classes
        .add_class(class_hash4, ApiContractClass::DeprecatedContractClass(class1.clone()));
    state_reader2.maybe_pending_data = Some(PendingData {
        storage_diffs: indexmap!(
            address0 => vec![StorageEntry{key: storage_key0, value: storage_value1}],
            address2 => vec![StorageEntry{key: storage_key0, value: storage_value2}],
        ),
        deployed_contracts: vec![DeployedContract { address: address2, class_hash: class_hash2 }],
        declared_classes: vec![DeclaredClassHashEntry {
            class_hash: class_hash2,
            compiled_class_hash: compiled_class_hash2,
        }],
        nonces: indexmap!(
            address2 => nonce1,
        ),
        classes: pending_classes,
        ..Default::default()
    });

    assert_eq!(state_reader2.get_storage_at(address0, storage_key0).unwrap(), storage_value1);
    assert_eq!(state_reader2.get_storage_at(address2, storage_key0).unwrap(), storage_value2);
    assert_eq!(state_reader2.get_class_hash_at(address0).unwrap(), class_hash0);
    assert_eq!(state_reader2.get_class_hash_at(address2).unwrap(), class_hash2);
    assert_eq!(state_reader2.get_compiled_class_hash(class_hash0).unwrap(), compiled_class_hash0);
    assert_eq!(state_reader2.get_compiled_class_hash(class_hash2).unwrap(), compiled_class_hash2);
    assert_eq!(state_reader2.get_nonce_at(address0).unwrap(), nonce0);
    assert_eq!(state_reader2.get_nonce_at(address2).unwrap(), nonce1);
    assert_eq!(state_reader2.get_compiled_class(class_hash0).unwrap(), blockifier_casm0);
    assert_eq!(state_reader2.get_compiled_class(class_hash2).unwrap(), blockifier_casm2);
    // Test that an error is returned if we only got the class without the casm.
    state_reader2.get_compiled_class(class_hash3).unwrap_err();
    // Test that if the class is deprecated it is returned.
    assert_eq!(
        state_reader2.get_compiled_class(class_hash4).unwrap(),
        RunnableCompiledClass::V0(CompiledClassV0::try_from(class1).unwrap())
    );

    // Test get_class_hash_at when the class is replaced.
    if let Some(pending_data) = &mut state_reader2.maybe_pending_data {
        pending_data.replaced_classes = vec![
            ReplacedClass { address: address0, class_hash: class_hash3 },
            ReplacedClass { address: address2, class_hash: class_hash3 },
        ];
    }
    assert_eq!(state_reader2.get_class_hash_at(address0).unwrap(), class_hash3);
    assert_eq!(state_reader2.get_class_hash_at(address2).unwrap(), class_hash3);
}

// Make sure we have the arbitrary precision feature of serde_json.
#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}

fn set_execution_state_reader(
    class_manager_client: SharedClassManagerClient,
    block_number: u64,
) -> (ExecutionStateReader, StorageWriter) {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let run_time_handle = tokio::runtime::Runtime::new().unwrap().handle().clone();

    (
        ExecutionStateReader {
            storage_reader,
            state_number: StateNumber::unchecked_right_after_block(BlockNumber(block_number)),
            maybe_pending_data: None,
            missing_compiled_class: Cell::new(None),
            class_manager_handle: Some((class_manager_client, run_time_handle)),
        },
        storage_writer,
    )
}

#[test]
fn get_compiled_class() {
    let casm_contract_class = CasmContractClass {
        compiler_version: "0.0.0".to_string(),
        prime: Default::default(),
        bytecode: Default::default(),
        bytecode_segment_lengths: Default::default(),
        hints: Default::default(),
        pythonic_hints: Default::default(),
        entry_points_by_type: Default::default(),
    };
    let expected_result = casm_contract_class.clone();

    let mut mock_class_manager_client = MockClassManagerClient::new();
    mock_class_manager_client.expect_get_executable().returning(move |_| {
        Ok(Some(ContractClass::V1((casm_contract_class.clone(), SierraVersion::default()))))
    });
    let shared_mock_class_manager_client = Arc::new(mock_class_manager_client);

    // Set execution state reader with state_number to be right after block 0.
    let (mut exec_state_reader, mut storage_writer) =
        set_execution_state_reader(shared_mock_class_manager_client.clone(), 0);

    // Prepare storage so class with hash 0x2 is declared in block 0 and class with hash 0x3 is
    // declared in block 1.
    let class_hash_0x2 = class_hash!("0x2");
    let class_hash_0x3 = class_hash!("0x3");

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .append_state_diff(
            BlockNumber(0),
            ThinStateDiff {
                declared_classes: indexmap!(class_hash_0x2 => compiled_class_hash!(1_u8)),
                ..Default::default()
            },
        )
        .unwrap()
        .append_header(
            BlockNumber(1),
            &BlockHeader { block_hash: BlockHash(felt!(1_u128)), ..Default::default() },
        )
        .unwrap()
        .append_state_diff(
            BlockNumber(1),
            ThinStateDiff {
                declared_classes: indexmap!(class_hash_0x3 => compiled_class_hash!(2_u8)),
                ..Default::default()
            },
        )
        .unwrap()
        .commit()
        .unwrap();

    // At state_number right after block 0, class with hash 0x2 is already declared.
    let result = exec_state_reader.get_compiled_class(class_hash_0x2).unwrap();
    assert_eq!(
        result,
        RunnableCompiledClass::V1(
            (expected_result.clone(), SierraVersion::default()).try_into().unwrap()
        )
    );

    // Class with hash 0x3 is not yet declared.
    let result = exec_state_reader.get_compiled_class(class_hash_0x3);
    assert_matches!(result, Err(StateError::UndeclaredClassHash(hash)) if hash == class_hash_0x3);

    // Adjust execution state reader state_number to be right after block 1.
    exec_state_reader.state_number = StateNumber::unchecked_right_after_block(BlockNumber(1));

    // Class with hash 0x2 is still declared.
    let result = exec_state_reader.get_compiled_class(class_hash_0x2).unwrap();
    assert_eq!(
        result,
        RunnableCompiledClass::V1(
            (expected_result.clone(), SierraVersion::default()).try_into().unwrap()
        )
    );

    // And class with hash 0x3 is declared too.
    let result = exec_state_reader.get_compiled_class(class_hash_0x3).unwrap();
    assert_eq!(
        result,
        RunnableCompiledClass::V1((expected_result, SierraVersion::default()).try_into().unwrap())
    );
}

#[test]
fn get_non_existing_compiled_class() {
    let mut mock_class_manager_client = MockClassManagerClient::new();
    mock_class_manager_client.expect_get_executable().returning(move |_| Ok(None));

    let (exec_state_reader, _) = set_execution_state_reader(Arc::new(mock_class_manager_client), 0);

    let class_hash = class_hash!("0x2");
    let result = exec_state_reader.get_compiled_class(class_hash);
    assert_matches!(result, Err(StateError::UndeclaredClassHash(hash)) if hash == class_hash);
}

#[test]
fn get_compiled_class_with_error() {
    let internal_err_msg = "Mock Error";
    let mut mock_class_manager_client = MockClassManagerClient::new();
    mock_class_manager_client.expect_get_executable().returning(move |_| {
        Err(ClassManagerClientError::ClassManagerError(ClassManagerError::Client(
            internal_err_msg.to_string(),
        )))
    });

    let (exec_state_reader, _) = set_execution_state_reader(Arc::new(mock_class_manager_client), 0);

    let result = exec_state_reader.get_compiled_class(class_hash!("0x2"));
    let expected_err_msg = format!("Internal client error: {internal_err_msg}");
    assert_matches!(result, Err(StateError::StateReadError(err_str)) if err_str == expected_err_msg);
}
