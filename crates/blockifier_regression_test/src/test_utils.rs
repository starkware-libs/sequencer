use blockifier::context::FeeTokenAddresses;
use papyrus_execution::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::{contract_address, felt, patricia_key};

pub fn get_fee_token_addresses() -> FeeTokenAddresses {
    FeeTokenAddresses {
        strk_fee_token_address: contract_address!(STRK_FEE_CONTRACT_ADDRESS),
        eth_fee_token_address: contract_address!(ETH_FEE_CONTRACT_ADDRESS),
    }
}
