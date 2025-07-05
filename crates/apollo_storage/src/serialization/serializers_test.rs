use std::env;
use std::fmt::Debug;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use apollo_test_utils::{get_rng, GetTestInstance};
use cairo_lang_casm::hints::CoreHintBase;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::hash::StarkHash;
use starknet_api::state::StorageKey;
use starknet_api::test_utils::{path_in_resources, read_json_file};
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
    let casm: CasmContractClass = read_json_file("erc20_compiled_contract_class.json");
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

const CASM_SERIALIZATION_REGRESSION_FILES: [(&str, &str); 3] = [
    ("openzeppelin_account.json", "openzeppelin_account.bin"),
    ("ERC20.json", "ERC20.bin"),
    ("libfuncs_full_coverage.json", "libfuncs_full_coverage.bin"),
];

const FIX_SUGGESTION: &str = "Consider re-generating the hardcoded binary files if you're ok with \
                              the serialization changing by re-running the test with the env var \
                              FIX=1. (Note that this should probably increase the major storage \
                              version.)";

#[test]
fn casm_serialization_regression() {
    let fix = env::var("FIX").unwrap_or_else(|_| "0".to_string());
    if fix == "1" {
        fix_casm_regression_files()
    }

    for (json_file_name, bin_file_name) in CASM_SERIALIZATION_REGRESSION_FILES {
        let json_path = format!("casm/{json_file_name}");
        let json_casm: CasmContractClass = read_json_file(&json_path);
        let mut serialized: Vec<u8> = Vec::new();
        json_casm
            .serialize_into(&mut serialized)
            .expect("Failed to serialize casm file: {json_file_name}");
        let bin_path = path_in_resources(Path::new("casm").join(bin_file_name));
        let mut bin_file = File::open(bin_path)
            .expect("Failed to open bin file: {bin_file_name}\n{FIX_SUGGESTION}");
        let mut regression_casm_bytes = Vec::new();
        bin_file
            .read_to_end(&mut regression_casm_bytes)
            .expect("Failed to read bin file: {bin_file_name}\n{FIX_SUGGESTION}");
        assert_eq!(
            regression_casm_bytes, serialized,
            "Serializing the casm gave a result different from the hardcoded \
             serialization.\n{FIX_SUGGESTION}"
        );
    }
}

#[test]
fn casm_deserialization_regression() {
    let fix = env::var("FIX").unwrap_or_else(|_| "0".to_string());
    if fix == "1" {
        fix_casm_regression_files()
    }

    for (json_file_name, bin_file_name) in CASM_SERIALIZATION_REGRESSION_FILES {
        let mut regression_casm_file =
            File::open(path_in_resources(Path::new("casm").join(bin_file_name)))
                .expect("Failed to open bin file: {bin_file_name}\n{FIX_SUGGESTION}");
        let mut regression_casm_bytes = Vec::new();
        regression_casm_file
            .read_to_end(&mut regression_casm_bytes)
            .expect("Failed to read bin file: {bin_file_name}\n{FIX_SUGGESTION}");
        let regression_casm =
            CasmContractClass::deserialize_from(&mut regression_casm_bytes.as_slice())
                .expect("Failed to deserialize casm file: {casm_file}.");
        let json_path = format!("casm/{json_file_name}");
        let json_casm: CasmContractClass = read_json_file(&json_path);
        assert_eq!(
            regression_casm, json_casm,
            "Deserializing the hardcoded serialization gave a different
result.\n{FIX_SUGGESTION}"
        );
    }
}

fn fix_casm_regression_files() {
    for (json_file_name, bin_file_name) in CASM_SERIALIZATION_REGRESSION_FILES {
        let json_path = format!("casm/{json_file_name}");
        let json_casm: CasmContractClass = read_json_file(&json_path);
        let mut serialized: Vec<u8> = Vec::new();
        json_casm.serialize_into(&mut serialized).unwrap();
        let casm_bytes = serialized.into_boxed_slice();
        let mut hardcoded_file =
            File::create(path_in_resources(Path::new("casm").join(bin_file_name)))
                .expect("Failed to create bin file {bin_file_name}\n");
        hardcoded_file.write_all(&casm_bytes).unwrap();
    }
}
