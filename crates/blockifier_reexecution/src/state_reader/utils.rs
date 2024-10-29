use blockifier::context::{ChainInfo, FeeTokenAddresses};
use indexmap::IndexMap;
use papyrus_execution::{eth_fee_contract_address, strk_fee_contract_address};
use starknet_api::core::ChainId;
use starknet_gateway::config::RpcStateReaderConfig;

pub const RPC_NODE_URL: &str = "https://free-rpc.nethermind.io/mainnet-juno/";
pub const JSON_RPC_VERSION: &str = "2.0";

/// Returns the fee token addresses of mainnet.
pub fn get_fee_token_addresses() -> FeeTokenAddresses {
    FeeTokenAddresses {
        strk_fee_token_address: strk_fee_contract_address(),
        eth_fee_token_address: eth_fee_contract_address(),
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
