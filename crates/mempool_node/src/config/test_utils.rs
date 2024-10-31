use std::vec::Vec; // Used by #[gen_field_names_fn].

use papyrus_proc_macros::gen_field_names_fn;
use starknet_api::core::ChainId;

/// Required parameters utility struct.
#[gen_field_names_fn]
pub struct RequiredParams {
    pub chain_id: ChainId,
}

impl RequiredParams {
    pub fn create_for_testing() -> Self {
        Self { chain_id: ChainId::create_for_testing() }
    }
}
