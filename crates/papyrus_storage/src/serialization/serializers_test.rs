use std::env;
use std::fmt::Debug;
use std::fs::{read_to_string, File};
use std::io::{Read, Write};
use std::path::Path;

use cairo_lang_casm::hints::CoreHintBase;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use papyrus_test_utils::{get_rng, read_json_file, GetTestInstance};
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::hash::StarkHash;
use starknet_api::state::StorageKey;
use starknet_api::transaction::TransactionOffsetInBlock;

use crate::db::serialization::StorageSerde;

pub trait StorageSerdeTest: StorageSerde {
    fn storage_serde_test();
}

// Implements the [`storage_serde_test`] function for every type that
// implements the [`StorageSerde`] and [`GetTestInstance`] traits.
impl<T: StorageSerde + GetTestInstance + Eq + Debug> StorageSerdeTest for T {
    fn storage_serde_test() {
        let mut rng = get_rng();
        let item = T::get_test_instance(&mut rng);
        let mut serialized: Vec<u8> = Vec::new();
        item.serialize_into(&mut serialized).unwrap();
        let bytes = serialized.into_boxed_slice();
        let deserialized = T::deserialize_from(&mut bytes.as_ref());
        assert_eq!(item, deserialized.unwrap());
    }
}

// Tests all types that implement the [`StorageSerde`] trait
// via the [`auto_storage_serde`] macro.
macro_rules! create_storage_serde_test {
    ($name:ident) => {
        paste::paste! {
            #[test]
            fn [<"storage_serde_test" _$name:snake>]() {
                $name::storage_serde_test()
            }
        }
    };
}
pub(crate) use create_storage_serde_test;

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for types not supported
// by the macro [`impl_get_test_instance`] and calls the [`create_test`]
// macro to create the tests for them.
////////////////////////////////////////////////////////////////////////
create_storage_serde_test!(bool);
create_storage_serde_test!(ContractAddress);
create_storage_serde_test!(StarkHash);
create_storage_serde_test!(StorageKey);
create_storage_serde_test!(u8);
create_storage_serde_test!(usize);
create_storage_serde_test!(BlockNumber);
create_storage_serde_test!(TransactionOffsetInBlock);

#[test]
fn transaction_offset_in_block_serialization_order() {
    let offset_1 = TransactionOffsetInBlock(1);
    let offset_256 = TransactionOffsetInBlock(256);
    let mut serialized_1 = Vec::new();
    offset_1.serialize_into(&mut serialized_1).unwrap();
    let mut serialized_256 = Vec::new();
    offset_256.serialize_into(&mut serialized_256).unwrap();
    assert!(serialized_256 > serialized_1);
}

#[test]
fn transaction_offset_in_block_serialization_max_value() {
    let item = TransactionOffsetInBlock((1 << 24) - 1);
    let mut buf = Vec::new();
    item.serialize_into(&mut buf).unwrap();
    let res = TransactionOffsetInBlock::deserialize_from(&mut buf.as_slice()).unwrap();
    assert_eq!(res, item);
}

#[test]
fn block_number_endianness() {
    let bn_255 = BlockNumber(255);
    let mut serialized: Vec<u8> = Vec::new();
    bn_255.serialize_into(&mut serialized).unwrap();
    let bytes_255 = serialized.into_boxed_slice();
    let deserialized = BlockNumber::deserialize_from(&mut bytes_255.as_ref());
    assert_eq!(bn_255, deserialized.unwrap());

    let bn_256 = BlockNumber(256);
    let mut serialized: Vec<u8> = Vec::new();
    bn_256.serialize_into(&mut serialized).unwrap();
    let bytes_256 = serialized.into_boxed_slice();
    let deserialized = BlockNumber::deserialize_from(&mut bytes_256.as_ref());
    assert_eq!(bn_256, deserialized.unwrap());

    assert!(bytes_255 < bytes_256);
}

// Make sure that the [`Hint`] schema is not modified. If it is, its encoding might change and a
// storage migration is needed.
#[test]
fn hint_modified() {
    // Only CoreHintBase is being used in programs (StarknetHint is for tests).
    let hint_schema = schemars::schema_for!(CoreHintBase);
    insta::assert_yaml_snapshot!(hint_schema);
}

// Tests the persistent encoding of the hints of an ERC20 contract.
// Each snapshot filename contains the hint's index in the origin casm file, so that a failure in
// the assertion of a file can lead to the hint that caused it.
#[test]
fn hints_regression() {
    let casm = serde_json::from_value::<CasmContractClass>(read_json_file(
        "erc20_compiled_contract_class.json",
    ))
    .unwrap();
    for hint in casm.hints.iter() {
        let mut encoded_hint: Vec<u8> = Vec::new();
        hint.serialize_into(&mut encoded_hint)
            .unwrap_or_else(|_| panic!("Failed to serialize hint {hint:?}"));
        insta::assert_yaml_snapshot!(format!("hints_regression_hint_{}", hint.0), encoded_hint);
    }
}

#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}
const SERIALIZATION_REGRESSION_FILES: [&str; 3] = ["account", "ERC20", "large_contract"];

#[test]
fn serialization_regression() {
    let fix = env::var("FIX").unwrap_or_else(|_| "0".to_string());
    if fix == "1" {
        fix_serialization_regression()
    }

    let resources_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("resources");
    for casm_file in SERIALIZATION_REGRESSION_FILES {
        let json_str =
            read_to_string(resources_path.join("casm").join(format!("{}.json", casm_file)))
                .unwrap_or_else(|err| panic!("Failed to read casm file: {casm_file}\n {err}"));
        let casm = serde_json::from_str::<CasmContractClass>(&json_str)
            .unwrap_or_else(|err| panic!("Failed to deserialize casm file: {casm_file}\n {err}"));
        let mut serialized: Vec<u8> = Vec::new();
        casm.serialize_into(&mut serialized)
            .unwrap_or_else(|err| panic!("Failed to serialize casm file: {casm_file}\n {err}"));
        let mut bin_file =
            File::open(resources_path.join("casm").join(format!("{}.bin", casm_file)))
                .unwrap_or_else(|err| panic!("Failed to open bin file: {casm_file}\n {err}"));
        let mut buffer = Vec::new();
        bin_file
            .read_to_end(&mut buffer)
            .unwrap_or_else(|err| panic!("Failed to read bin file: {casm_file}\n {err}"));
        assert_eq!(
            buffer, serialized,
            "Assertion failed duo to serialization mismatch.\n Consider re-generating the binary \
             files by running with FIX=1."
        );
    }
}

#[test]
fn deserialization_regression() {
    let fix = env::var("FIX").unwrap_or_else(|_| "0".to_string());
    if fix == "1" {
        fix_serialization_regression()
    }

    let resources_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("resources");
    for casm_file in SERIALIZATION_REGRESSION_FILES {
        let mut bin_file =
            File::open(resources_path.join("casm").join(format!("{}.bin", casm_file)))
                .unwrap_or_else(|err| panic!("Failed to open bin file: {casm_file}\n {err}"));
        let mut bin = Vec::new();
        bin_file
            .read_to_end(&mut bin)
            .unwrap_or_else(|err| panic!("Failed to read bin file: {casm_file}\n {err}"));
        let bin_casm = CasmContractClass::deserialize_from(&mut bin.as_slice())
            .unwrap_or_else(|| panic!("Failed to deserialize casm file: {casm_file}."));
        let json_str =
            read_to_string(resources_path.join("casm").join(format!("{}.json", casm_file)))
                .unwrap_or_else(|err| panic!("Failed to read casm file: {casm_file}\n {err}"));
        let json_casm = serde_json::from_str::<CasmContractClass>(&json_str)
            .unwrap_or_else(|err| panic!("Failed to deserialize casm file: {casm_file}\n {err}"));
        assert_eq!(
            bin_casm, json_casm,
            "Assertion failed duo to serialization mismatch.\n Consider re-generating the binary \
             files by running with FIX=1."
        );
    }
}

fn fix_serialization_regression() {
    let resources_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("resources");
    for casm_file in SERIALIZATION_REGRESSION_FILES {
        let path = resources_path.join("casm").join(format!("{}.json", casm_file));
        let json_str = read_to_string(path)
            .unwrap_or_else(|err| panic!("Failed to read casm file: {casm_file}\nError: {err}"));
        let casm = serde_json::from_str::<CasmContractClass>(&json_str).unwrap_or_else(|err| {
            panic!("Failed to deserialize casm file: {casm_file}\nError: {err}")
        });
        let mut serialized: Vec<u8> = Vec::new();
        casm.serialize_into(&mut serialized).unwrap();
        let bytes = serialized.into_boxed_slice();
        let mut bin = File::create(resources_path.join("casm").join(format!("{}.bin", casm_file)))
            .unwrap_or_else(|err| {
                panic!("Failed to create bin file for {casm_file}\nError: {err}")
            });
        bin.write_all(&bytes).unwrap();
    }
}
