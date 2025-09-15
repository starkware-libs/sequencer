use std::sync::Arc;

use assert_matches::assert_matches;
use rstest::rstest;
use serde::Deserialize;
use size_of::SizeOf;
use starknet_types_core::felt::Felt;

use crate::core::{ClassHash, CompiledClassHash, Nonce, PatriciaKey, TransactionHash};
use crate::deprecated_contract_class::{
    ConstructorType,
    ContractClassAbiEntry,
    FunctionAbiEntry,
    TypedParameter,
};
use crate::serde_utils::{
    bytes_from_hex_str,
    deserialize_optional_contract_class_abi_entry_vector,
    hex_str_from_bytes,
    BytesAsHex,
    InnerDeserializationError,
};
use crate::transaction::fields::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    PaymasterData,
    TransactionSignature,
};

#[test]
fn hex_str_from_bytes_scenarios() {
    // even length.
    assert_eq!(hex_str_from_bytes::<1, true>([106]), "0x6a");

    // odd length.
    assert_eq!(hex_str_from_bytes::<1, true>([6]), "0x6");

    // Remove padding.
    assert_eq!(hex_str_from_bytes::<2, true>([0, 6]), "0x6");

    // Non-prefixed.
    assert_eq!(hex_str_from_bytes::<2, false>([13, 162]), "da2");
}

#[test]
fn hex_str_from_bytes_zero() {
    // Prefixed.
    assert_eq!(hex_str_from_bytes::<3, true>([0, 0, 0]), "0x0");

    // Non-prefixed.
    assert_eq!(hex_str_from_bytes::<2, false>([0, 0]), "0");
}

#[test]
fn bytes_from_hex_str_scenarios() {
    // even length.
    let hex_str = "0x6a";
    let res = bytes_from_hex_str::<1, true>(hex_str).unwrap();
    assert_eq!(res, [106]);

    // odd length.
    let hex_str = "0x6";
    let res = bytes_from_hex_str::<1, true>(hex_str).unwrap();
    assert_eq!(res, [6]);

    // No prefix.
    let hex_str = "6";
    let res = bytes_from_hex_str::<1, false>(hex_str).unwrap();
    assert_eq!(res, [6]);
}

#[test]
fn bytes_from_hex_str_padding() {
    // even length.
    let hex_str = "0xda2b";
    let res = bytes_from_hex_str::<4, true>(hex_str).unwrap();
    assert_eq!(res, [0, 0, 218, 43]);

    // odd length.
    let hex_str = "0xda2";
    let res = bytes_from_hex_str::<4, true>(hex_str).unwrap();
    assert_eq!(res, [0, 0, 13, 162]);
}

#[test]
fn bytes_from_hex_str_errors() {
    // Short buffer.
    let hex_str = "0xda2b";
    let err = bytes_from_hex_str::<1, true>(hex_str);
    assert_matches!(err, Err(InnerDeserializationError::BadInput { expected_byte_count: 1, .. }));

    // Invalid hex char.
    let err = bytes_from_hex_str::<1, false>("1z");
    assert_matches!(
        err,
        Err(InnerDeserializationError::FromHex(hex::FromHexError::InvalidHexCharacter {
            c: 'z',
            index: 1
        }))
    );

    // Missing prefix.
    let err = bytes_from_hex_str::<2, true>("11");
    assert_matches!(err, Err(InnerDeserializationError::MissingPrefix { .. }));

    // Unneeded prefix.
    let err = bytes_from_hex_str::<2, false>("0x11");
    assert_matches!(
        err,
        Err(InnerDeserializationError::FromHex(hex::FromHexError::InvalidHexCharacter {
            c: 'x',
            index: 1
        }))
    );
}

#[test]
fn hex_as_bytes_serde_prefixed() {
    let hex_as_bytes = BytesAsHex::<3, true>([1, 2, 3]);
    assert_eq!(
        hex_as_bytes,
        serde_json::from_str(&serde_json::to_string(&hex_as_bytes).unwrap()).unwrap()
    );
}

#[test]
fn hex_as_bytes_serde_not_prefixed() {
    let hex_as_bytes = BytesAsHex::<3, false>([1, 2, 3]);
    assert_eq!(
        hex_as_bytes,
        serde_json::from_str(&serde_json::to_string(&hex_as_bytes).unwrap()).unwrap()
    );
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
struct DummyContractClass {
    #[serde(default, deserialize_with = "deserialize_optional_contract_class_abi_entry_vector")]
    pub abi: Option<Vec<ContractClassAbiEntry>>,
}

#[test]
fn deserialize_valid_optional_contract_class_abi_entry_vector() {
    let json = r#"
    {
        "abi":
        [
            {
                "inputs": [
                    {
                        "name": "implementation",
                        "type": "felt"
                    }
                ],
                "name": "constructor",
                "outputs": [],
                "type": "constructor"
            }
        ]
    }
    "#;
    let res: DummyContractClass = serde_json::from_str(json).unwrap();
    assert_eq!(
        res,
        DummyContractClass {
            abi: Some(vec![ContractClassAbiEntry::Constructor(FunctionAbiEntry::<
                ConstructorType,
            > {
                name: "constructor".to_string(),
                inputs: vec![TypedParameter {
                    name: "implementation".to_string(),
                    r#type: "felt".to_string(),
                }],
                outputs: vec![],
                state_mutability: None,
                r#type: ConstructorType::Constructor,
            })])
        }
    );
}

#[test]
fn deserialize_optional_contract_class_abi_entry_vector_junk() {
    let json = r#"
    {
        "abi": "Junk"
    }
    "#;
    let res: DummyContractClass = serde_json::from_str(json).unwrap();
    assert_eq!(res, DummyContractClass { abi: None });
}

#[test]
fn deserialize_optional_contract_class_abi_entry_vector_none() {
    let json = r#"
    {
    }
    "#;
    let res: DummyContractClass = serde_json::from_str(json).unwrap();
    assert_eq!(res, DummyContractClass { abi: None });
}

#[rstest]
#[case(ContractAddressSalt::default(), 32)]
#[case(ClassHash::default(), 32)]
#[case(CompiledClassHash::default(), 32)]
#[case(TransactionHash::default(), 32)]
#[case(PatriciaKey::default(), 32)]
#[case(Nonce::default(), 32)]
#[case(
    PaymasterData(vec![Felt::ONE, Felt::TWO, Felt::THREE]),
    32 * 3 + std::mem::size_of::<Vec<Felt>>()
)]
#[case(
    AccountDeploymentData(vec![Felt::ONE, Felt::TWO, Felt::THREE]),
    32 * 3 + std::mem::size_of::<Vec<Felt>>()
)]
#[case(
    Calldata(Arc::new(vec![Felt::ONE, Felt::TWO, Felt::THREE])),
    32 * 3 + std::mem::size_of::<Arc<Vec<Felt>>>()
)]
#[case(
    TransactionSignature(Arc::new(vec![Felt::ONE, Felt::TWO, Felt::THREE])),
    32 * 3 + std::mem::size_of::<Arc<Vec<Felt>>>()
)]
fn test_size_of_felt_wrappers<T: SizeOf>(#[case] data: T, #[case] expected_size: usize) {
    let total_bytes = data.size_of().total_bytes();
    assert_eq!(total_bytes, expected_size);
}
