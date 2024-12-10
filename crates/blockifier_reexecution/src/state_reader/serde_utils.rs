use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;
use starknet_api::transaction::{
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    Transaction,
};

use crate::state_reader::errors::ReexecutionResult;

/// In old transaction, the resource bounds names are lowercase.
/// need to convert to uppercase for deserialization to work.
pub fn upper_case_resource_bounds_names(raw_transaction: &mut Value) {
    let resource_bounds = raw_transaction
        .get_mut("resource_bounds")
        .expect("tx should contain resource_bounds field")
        .as_object_mut()
        .expect("resource_bounds should be an object");

    if let Some(l1_gas_value) = resource_bounds.remove("l1_gas") {
        resource_bounds.insert("L1_GAS".to_string(), l1_gas_value);

        let l2_gas_value = resource_bounds
            .remove("l2_gas")
            .expect("If tx contains l1_gas, it should contain l2_gas");
        resource_bounds.insert("L2_GAS".to_string(), l2_gas_value);
    }

    if let Some(l1_data_gas_value) = resource_bounds.remove("l1_data_gas") {
        resource_bounds.insert("L1_DATA_GAS".to_string(), l1_data_gas_value);
    }
}

pub fn deserialize_transaction_json_to_starknet_api_tx(
    mut raw_transaction: Value,
) -> serde_json::Result<Transaction> {
    let tx_type: String = serde_json::from_value(raw_transaction["type"].clone())?;
    let tx_version: String = serde_json::from_value(raw_transaction["version"].clone())?;

    match (tx_type.as_str(), tx_version.as_str()) {
        ("INVOKE", "0x0") => {
            Ok(Transaction::Invoke(InvokeTransaction::V0(serde_json::from_value(raw_transaction)?)))
        }
        ("INVOKE", "0x1") => {
            Ok(Transaction::Invoke(InvokeTransaction::V1(serde_json::from_value(raw_transaction)?)))
        }
        ("INVOKE", "0x3") => {
            // In old invoke v3 transaction, the resource bounds names are lowercase.
            upper_case_resource_bounds_names(&mut raw_transaction);
            Ok(Transaction::Invoke(InvokeTransaction::V3(serde_json::from_value(raw_transaction)?)))
        }
        ("DEPLOY_ACCOUNT", "0x1") => Ok(Transaction::DeployAccount(DeployAccountTransaction::V1(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DEPLOY_ACCOUNT", "0x3") => {
            // In old deploy account v3 transaction, the resource bounds names are lowercase.
            upper_case_resource_bounds_names(&mut raw_transaction);
            Ok(Transaction::DeployAccount(DeployAccountTransaction::V3(serde_json::from_value(
                raw_transaction,
            )?)))
        }
        ("DECLARE", "0x0") => Ok(Transaction::Declare(DeclareTransaction::V0(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DECLARE", "0x1") => Ok(Transaction::Declare(DeclareTransaction::V1(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DECLARE", "0x2") => Ok(Transaction::Declare(DeclareTransaction::V2(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DECLARE", "0x3") => {
            // In old declare v3 transaction, the resource bounds names are lowercase.
            upper_case_resource_bounds_names(&mut raw_transaction);
            Ok(Transaction::Declare(DeclareTransaction::V3(serde_json::from_value(
                raw_transaction,
            )?)))
        }
        ("L1_HANDLER", _) => Ok(Transaction::L1Handler(serde_json::from_value(raw_transaction)?)),
        (tx_type, tx_version) => Err(serde::de::Error::custom(format!(
            "unimplemented transaction type: {tx_type} version: {tx_version}"
        ))),
    }
}

// TODO(Aner): import the following functions instead, to reduce code duplication.
pub(crate) fn hashmap_from_raw<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    V: for<'de> Deserialize<'de>,
>(
    raw_object: &Value,
    vec_str: &str,
    key_str: &str,
    value_str: &str,
) -> ReexecutionResult<IndexMap<K, V>> {
    Ok(vec_to_hashmap::<K, V>(
        serde_json::from_value(raw_object[vec_str].clone())?,
        key_str,
        value_str,
    ))
}

pub(crate) fn nested_hashmap_from_raw<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VK: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VV: for<'de> Deserialize<'de>,
>(
    raw_object: &Value,
    vec_str: &str,
    key_str: &str,
    value_str: &str,
    inner_key_str: &str,
    inner_value_str: &str,
) -> ReexecutionResult<IndexMap<K, IndexMap<VK, VV>>> {
    Ok(vec_to_nested_hashmap::<K, VK, VV>(
        serde_json::from_value(raw_object[vec_str].clone())?,
        key_str,
        value_str,
        inner_key_str,
        inner_value_str,
    ))
}

pub(crate) fn vec_to_hashmap<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    V: for<'de> Deserialize<'de>,
>(
    vec: Vec<Value>,
    key_str: &str,
    value_str: &str,
) -> IndexMap<K, V> {
    vec.iter()
        .map(|element| {
            (
                serde_json::from_value(element[key_str].clone())
                    .expect("Key string doesn't match expected."),
                serde_json::from_value(element[value_str].clone())
                    .expect("Value string doesn't match expected."),
            )
        })
        .collect()
}

pub(crate) fn vec_to_nested_hashmap<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VK: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VV: for<'de> Deserialize<'de>,
>(
    vec: Vec<Value>,
    key_str: &str,
    value_str: &str,
    inner_key_str: &str,
    inner_value_str: &str,
) -> IndexMap<K, IndexMap<VK, VV>> {
    vec.iter()
        .map(|element| {
            (
                serde_json::from_value(element[key_str].clone()).expect("Couldn't deserialize key"),
                vec_to_hashmap(
                    serde_json::from_value(element[value_str].clone())
                        .expect("Couldn't deserialize value"),
                    inner_key_str,
                    inner_value_str,
                ),
            )
        })
        .collect()
}
