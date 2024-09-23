use blockifier::context::{ChainInfo, FeeTokenAddresses};
use papyrus_execution::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use starknet_api::core::{ChainId, ContractAddress, PatriciaKey};
use starknet_api::{contract_address, felt, patricia_key};
use starknet_gateway::config::RpcStateReaderConfig;

pub const RPC_NODE_URL: &str = "https://free-rpc.nethermind.io/mainnet-juno/";
pub const JSON_RPC_VERSION: &str = "2.0";

pub fn get_fee_token_addresses() -> FeeTokenAddresses {
    FeeTokenAddresses {
        strk_fee_token_address: contract_address!(STRK_FEE_CONTRACT_ADDRESS),
        eth_fee_token_address: contract_address!(ETH_FEE_CONTRACT_ADDRESS),
    }
}

pub fn get_rpc_state_reader_config() -> RpcStateReaderConfig {
    RpcStateReaderConfig {
        url: RPC_NODE_URL.to_string(),
        json_rpc_version: JSON_RPC_VERSION.to_string(),
    }
}

pub fn get_chain_info() -> ChainInfo {
    ChainInfo { chain_id: ChainId::Mainnet, fee_token_addresses: get_fee_token_addresses() }
}
