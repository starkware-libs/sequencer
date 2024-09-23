use blockifier::context::FeeTokenAddresses;
use starknet_api::contract_address;
use starknet_api::core::ContractAddress;
use starknet_api::patricia_key;
use starknet_api::core::PatriciaKey;
use starknet_api::felt;
use papyrus_execution::{ETH_FEE_CONTRACT_ADDRESS,STRK_FEE_CONTRACT_ADDRESS};

pub fn get_fee_token_addresses() -> FeeTokenAddresses {
    FeeTokenAddresses {
        strk_fee_token_address: contract_address!(STRK_FEE_CONTRACT_ADDRESS),
        eth_fee_token_address: contract_address!(ETH_FEE_CONTRACT_ADDRESS),
    }
}
