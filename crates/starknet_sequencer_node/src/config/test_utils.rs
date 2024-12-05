use std::vec::Vec; // Used by #[gen_field_names_fn].

use papyrus_proc_macros::gen_field_names_and_cli_args_fn;
use starknet_api::core::{ChainId, ContractAddress};

use crate::config::node_config::node_command;

/// Required parameters utility struct.
#[gen_field_names_and_cli_args_fn]
pub struct RequiredParams {
    pub chain_id: ChainId,
    pub eth_fee_token_address: ContractAddress,
    pub strk_fee_token_address: ContractAddress,
    pub validator_id: ContractAddress,
}

impl RequiredParams {
    pub fn create_for_testing() -> Self {
        Self {
            chain_id: ChainId::create_for_testing(),
            eth_fee_token_address: ContractAddress::from(2_u128),
            strk_fee_token_address: ContractAddress::from(3_u128),
            validator_id: ContractAddress::from(100_u128),
        }
    }
}

// Creates a vector of strings with the command name and required parameters that can be used as
// arguments to load a config.
pub fn create_test_config_load_args(required_params: RequiredParams) -> Vec<String> {
    let mut cli_args = vec![node_command().to_string()];
    cli_args.extend(required_params.cli_args());
    cli_args
}
