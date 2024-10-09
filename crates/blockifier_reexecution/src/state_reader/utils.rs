use blockifier::context::{ChainInfo, FeeTokenAddresses};
use blockifier::state::state_api::StateResult;
use indexmap::IndexMap;
use papyrus_execution::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use serde::Deserialize;
use serde_json::Value;
use starknet_api::core::{ChainId, ContractAddress, PatriciaKey};
use starknet_api::transaction::{
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    Transaction,
};
use starknet_api::{contract_address, felt, patricia_key};
use starknet_gateway::config::RpcStateReaderConfig;
use starknet_gateway::errors::serde_err_to_state_err;

pub const RPC_NODE_URL: &str = "https://free-rpc.nethermind.io/mainnet-juno/";
pub const JSON_RPC_VERSION: &str = "2.0";

/// Returns the fee token addresses of mainnet.
pub fn get_fee_token_addresses() -> FeeTokenAddresses {
    FeeTokenAddresses {
        strk_fee_token_address: contract_address!(STRK_FEE_CONTRACT_ADDRESS),
        eth_fee_token_address: contract_address!(ETH_FEE_CONTRACT_ADDRESS),
    }
}

/// Returns the RPC state reader configuration with the constants RPC_NODE_URL and JSON_RPC_VERSION.
pub fn get_rpc_state_reader_config() -> RpcStateReaderConfig {
    RpcStateReaderConfig {
        url: RPC_NODE_URL.to_string(),
        json_rpc_version: JSON_RPC_VERSION.to_string(),
    }
}

/// Returns the chain info of mainnet.
pub fn get_chain_info() -> ChainInfo {
    ChainInfo { chain_id: ChainId::Mainnet, fee_token_addresses: get_fee_token_addresses() }
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
            let resource_bounds = raw_transaction
                .get_mut("resource_bounds")
                .expect("Invoke v3 tx should contain resource_bounds field")
                .as_object_mut()
                .expect("resource_bounds should be an object");

            // In old invoke v3 transaction, the resource bounds names are lowercase.
            // need to convert to uppercase for deserialization to work.
            if let Some(l1_gas_value) = resource_bounds.remove("l1_gas") {
                resource_bounds.insert("L1_GAS".to_string(), l1_gas_value);

                let l2_gas_value = resource_bounds
                    .remove("l2_gas")
                    .expect("If invoke v3 tx contains l1_gas, it should contain l2_gas");
                resource_bounds.insert("L2_GAS".to_string(), l2_gas_value);
            }

            Ok(Transaction::Invoke(InvokeTransaction::V3(serde_json::from_value(raw_transaction)?)))
        }
        ("DEPLOY_ACCOUNT", "0x1") => Ok(Transaction::DeployAccount(DeployAccountTransaction::V1(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DEPLOY_ACCOUNT", "0x3") => Ok(Transaction::DeployAccount(DeployAccountTransaction::V3(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DECLARE", "0x0") => Ok(Transaction::Declare(DeclareTransaction::V0(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DECLARE", "0x1") => Ok(Transaction::Declare(DeclareTransaction::V1(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DECLARE", "0x2") => Ok(Transaction::Declare(DeclareTransaction::V2(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("DECLARE", "0x3") => Ok(Transaction::Declare(DeclareTransaction::V3(
            serde_json::from_value(raw_transaction)?,
        ))),
        ("L1_HANDLER", _) => Ok(Transaction::L1Handler(serde_json::from_value(raw_transaction)?)),
        (tx_type, tx_version) => Err(serde::de::Error::custom(format!(
            "unimplemented transaction type: {tx_type} version: {tx_version}"
        ))),
    }
}

// TODO(Aner): import the following functions instead, to reduce code duplication.
pub(crate) fn disjoint_hashmap_union<K: std::hash::Hash + std::cmp::Eq, V>(
    map1: IndexMap<K, V>,
    map2: IndexMap<K, V>,
) -> IndexMap<K, V> {
    let expected_len = map1.len() + map2.len();
    let union_map: IndexMap<K, V> = map1.into_iter().chain(map2).collect();
    // verify union length is sum of lengths (disjoint union)
    assert_eq!(union_map.len(), expected_len, "Intersection of hashmaps is not empty.");
    union_map
}

pub(crate) fn hashmap_from_raw<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    V: for<'de> Deserialize<'de>,
>(
    raw_object: &Value,
    vec_str: &str,
    key_str: &str,
    value_str: &str,
) -> StateResult<IndexMap<K, V>> {
    Ok(vec_to_hashmap::<K, V>(
        serde_json::from_value(raw_object[vec_str].clone()).map_err(serde_err_to_state_err)?,
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
) -> StateResult<IndexMap<K, IndexMap<VK, VV>>> {
    Ok(vec_to_nested_hashmap::<K, VK, VV>(
        serde_json::from_value(raw_object[vec_str].clone()).map_err(serde_err_to_state_err)?,
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
