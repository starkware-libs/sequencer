use assert_matches::assert_matches;
use indexmap::IndexMap;

use super::{Builtin, ExecutionResources, Transaction, TransactionReceipt};
use crate::test_utils::read_resource::read_resource_file;

/// The builtin counter must serialize in insertion order (the Python feeder gateway does not sort
/// it), which is why it is an `IndexMap` rather than a `HashMap`.
#[test]
fn builtin_instance_counter_serializes_in_insertion_order() {
    let mut builtin_instance_counter = IndexMap::new();
    builtin_instance_counter.insert(Builtin::Poseidon, 1);
    builtin_instance_counter.insert(Builtin::RangeCheck, 2);
    builtin_instance_counter.insert(Builtin::Pedersen, 3);
    let execution_resources = ExecutionResources {
        n_steps: 0,
        builtin_instance_counter,
        n_memory_holes: 0,
        data_availability: None,
        total_gas_consumed: None,
    };

    let serialized = serde_json::to_string(&execution_resources).unwrap();
    let poseidon = serialized.find("poseidon_builtin").unwrap();
    let range_check = serialized.find("range_check_builtin").unwrap();
    let pedersen = serialized.find("pedersen_builtin").unwrap();
    assert!(
        poseidon < range_check && range_check < pedersen,
        "builtins serialized out of insertion order: {serialized}"
    );
}

#[test]
fn load_deploy_transaction_succeeds() {
    assert_matches!(
        serde_json::from_str::<Transaction>(&read_resource_file("reader/deploy_v0.json")),
        Ok(Transaction::Deploy(_))
    );
}

#[test]
fn load_invoke_transaction_succeeds() {
    assert_matches!(
        serde_json::from_str::<Transaction>(&read_resource_file("reader/invoke_v0.json")),
        Ok(Transaction::Invoke(_))
    );
}

#[test]
fn load_invoke_with_contract_address_transaction_succeeds() {
    let mut json_val: serde_json::Value =
        serde_json::from_str(&read_resource_file("reader/invoke_v0.json")).unwrap();
    let object = json_val.as_object_mut().unwrap();
    let sender_address_value = object.remove("sender_address").unwrap();
    object.insert("contract_address".to_string(), sender_address_value);
    assert_matches!(serde_json::from_value::<Transaction>(json_val), Ok(Transaction::Invoke(_)));
}

#[test]
fn load_l1_handler_transaction_succeeds() {
    assert_matches!(
        serde_json::from_str::<Transaction>(&read_resource_file("reader/l1_handler_v0.json")),
        Ok(Transaction::L1Handler(_))
    );
}

#[test]
fn load_declare_transaction_succeeds() {
    assert_matches!(
        serde_json::from_str::<Transaction>(&read_resource_file("reader/declare_v0.json")),
        Ok(Transaction::Declare(_))
    );
}

#[test]
fn load_transaction_succeeds() {
    for file_name in [
        "reader/deploy_v0.json",
        "reader/invoke_v0.json",
        // TODO(AvivG/ Meshi): Refactor proof_facts field with correct values, once program hash is
        // available.
        "reader/invoke_v3_client_side_proving.json",
        "reader/invoke_v3.json",
        "reader/declare_v0.json",
        "reader/declare_v3.json",
        "reader/deploy_account_v3.json",
    ] {
        let res = serde_json::from_str::<Transaction>(&read_resource_file(file_name));
        assert!(res.is_ok(), "filename: {file_name}, error: {res:?}");
    }
}

#[test]
fn load_transaction_unknown_field_fails() {
    for file_name in [
        "reader/deploy_v0.json",
        "reader/invoke_v0.json",
        "reader/declare_v0.json",
        "reader/deploy_account_v3.json",
    ] {
        let mut json_value: serde_json::Value =
            serde_json::from_str(&read_resource_file(file_name)).unwrap();
        json_value
            .as_object_mut()
            .unwrap()
            .insert("unknown_field".to_string(), serde_json::Value::Null);
        let json_str = serde_json::to_string(&json_value).unwrap();
        assert!(serde_json::from_str::<Transaction>(&json_str).is_err(), "filename: {file_name}");
    }
}

#[test]
fn load_transaction_wrong_type_fails() {
    for (file_name, new_wrong_type) in [
        // The transaction has a type that doesn't match the type it is paired with.
        ("reader/deploy_v0.json", "INVOKE_FUNCTION"),
        ("reader/invoke_v0.json", "DECLARE"),
        ("reader/declare_v0.json", "DEPLOY"),
        ("reader/deploy_account_v3.json", "INVOKE_FUNCTION"),
    ] {
        let mut json_value: serde_json::Value =
            serde_json::from_str(&read_resource_file(file_name)).unwrap();
        json_value
            .as_object_mut()
            .unwrap()
            .insert("type".to_string(), serde_json::Value::String(new_wrong_type.to_string()));
        let json_str = serde_json::to_string(&json_value).unwrap();
        assert!(serde_json::from_str::<Transaction>(&json_str).is_err(), "filename: {file_name}");
    }
}

#[test]
fn load_transaction_receipt_succeeds() {
    for file_name in [
        "reader/transaction_receipt.json",
        "reader/transaction_receipt_without_l1_to_l2.json",
        "reader/transaction_receipt_without_l1_to_l2_nonce.json",
    ] {
        serde_json::from_str::<TransactionReceipt>(&read_resource_file(file_name)).unwrap_or_else(
            |err| {
                panic!(
                    "Failed to deserialize transaction receipt. Filename: {file_name}. Error: \
                     {err}"
                )
            },
        );
    }
}

/// The Python feeder gateway serializes the `type` tag LAST in every transaction object; serde's
/// `#[serde(tag)]` would emit it first, so `Transaction` has a custom `Serialize`. Locks the tag
/// position for every variant and proves the custom impl round-trips losslessly.
#[test]
fn transaction_serializes_type_tag_last() {
    for (file_name, type_tag) in [
        ("reader/declare_v0.json", "DECLARE"),
        ("reader/declare_v3.json", "DECLARE"),
        ("reader/deploy_account_v3.json", "DEPLOY_ACCOUNT"),
        ("reader/deploy_v0.json", "DEPLOY"),
        ("reader/invoke_v0.json", "INVOKE_FUNCTION"),
        ("reader/invoke_v3.json", "INVOKE_FUNCTION"),
        ("reader/l1_handler_v0.json", "L1_HANDLER"),
    ] {
        let transaction: Transaction =
            serde_json::from_str(&read_resource_file(file_name)).unwrap();
        let serialized = serde_json::to_string(&transaction).unwrap();
        assert!(
            serialized.ends_with(&format!(r#""type":"{type_tag}"}}"#)),
            "`type` is not the last key for {file_name}: {serialized}"
        );

        let round_tripped: Transaction = serde_json::from_str(&serialized).unwrap();
        assert_eq!(round_tripped, transaction, "lossy serialization for {file_name}");
    }
}

/// Asserts the top-level keys of the serialized transaction appear in exactly the given order
/// (the live Python feeder gateway wire order; key sets and orders were captured live per
/// transaction family and version on 2026-06-03).
fn assert_serialized_key_order(transaction: &Transaction, expected_key_order: &[&str]) {
    let serialized = serde_json::to_string(transaction).unwrap();
    let mut key_positions = Vec::with_capacity(expected_key_order.len());
    for key in expected_key_order {
        let needle = format!(r#""{key}":"#);
        let position = serialized
            .find(&needle)
            .unwrap_or_else(|| panic!("missing key {key} in: {serialized}"));
        key_positions.push(position);
    }
    assert!(
        key_positions.windows(2).all(|pair| pair[0] < pair[1]),
        "keys serialized out of live wire order (expected {expected_key_order:?}): {serialized}"
    );
}

#[test]
fn deploy_serializes_in_live_wire_order() {
    let transaction: Transaction =
        serde_json::from_str(&read_resource_file("reader/deploy_v0.json")).unwrap();
    assert_serialized_key_order(
        &transaction,
        &[
            "transaction_hash",
            "version",
            "contract_address",
            "contract_address_salt",
            "class_hash",
            "constructor_calldata",
            "type",
        ],
    );
}

#[test]
fn l1_handler_serializes_in_live_wire_order() {
    let transaction: Transaction =
        serde_json::from_str(&read_resource_file("reader/l1_handler_v0.json")).unwrap();
    assert_serialized_key_order(
        &transaction,
        &[
            "transaction_hash",
            "version",
            "contract_address",
            "entry_point_selector",
            "nonce",
            "calldata",
            "type",
        ],
    );
}

#[test]
fn invoke_serializes_in_live_wire_order_per_version() {
    let invoke_v0: Transaction =
        serde_json::from_str(&read_resource_file("reader/invoke_v0.json")).unwrap();
    assert_serialized_key_order(
        &invoke_v0,
        &[
            "transaction_hash",
            "version",
            "max_fee",
            "signature",
            "entry_point_selector",
            "calldata",
            "contract_address",
            "type",
        ],
    );
    // V0 must serve the legacy address key, not the modern one.
    assert!(!serde_json::to_string(&invoke_v0).unwrap().contains("sender_address"));

    let invoke_v1: Transaction = serde_json::from_str(
        r#"{"transaction_hash": "0x1", "version": "0x1", "max_fee": "0x2", "signature": [],
            "nonce": "0x0", "sender_address": "0x3", "calldata": [], "type": "INVOKE_FUNCTION"}"#,
    )
    .unwrap();
    assert_serialized_key_order(
        &invoke_v1,
        &[
            "transaction_hash",
            "version",
            "max_fee",
            "signature",
            "nonce",
            "sender_address",
            "calldata",
            "type",
        ],
    );

    let invoke_v3: Transaction =
        serde_json::from_str(&read_resource_file("reader/invoke_v3.json")).unwrap();
    assert_serialized_key_order(
        &invoke_v3,
        &[
            "transaction_hash",
            "version",
            "signature",
            "nonce",
            "nonce_data_availability_mode",
            "fee_data_availability_mode",
            "resource_bounds",
            "tip",
            "paymaster_data",
            "sender_address",
            "calldata",
            "account_deployment_data",
            "type",
        ],
    );
}

#[test]
fn declare_serializes_in_live_wire_order_per_version() {
    let declare_v0: Transaction =
        serde_json::from_str(&read_resource_file("reader/declare_v0.json")).unwrap();
    assert_serialized_key_order(
        &declare_v0,
        &[
            "transaction_hash",
            "version",
            "max_fee",
            "signature",
            "nonce",
            "class_hash",
            "sender_address",
            "type",
        ],
    );

    // V2 inserts compiled_class_hash between class_hash and sender_address (verified live).
    let declare_v2: Transaction = serde_json::from_str(
        r#"{"transaction_hash": "0x1", "version": "0x2", "max_fee": "0x2", "signature": [],
            "nonce": "0x0", "class_hash": "0x3", "compiled_class_hash": "0x4",
            "sender_address": "0x5", "type": "DECLARE"}"#,
    )
    .unwrap();
    assert_serialized_key_order(
        &declare_v2,
        &[
            "transaction_hash",
            "version",
            "max_fee",
            "signature",
            "nonce",
            "class_hash",
            "compiled_class_hash",
            "sender_address",
            "type",
        ],
    );

    let declare_v3: Transaction =
        serde_json::from_str(&read_resource_file("reader/declare_v3.json")).unwrap();
    assert_serialized_key_order(
        &declare_v3,
        &[
            "transaction_hash",
            "version",
            "signature",
            "nonce",
            "nonce_data_availability_mode",
            "fee_data_availability_mode",
            "resource_bounds",
            "tip",
            "paymaster_data",
            "sender_address",
            "class_hash",
            "compiled_class_hash",
            "account_deployment_data",
            "type",
        ],
    );
}
