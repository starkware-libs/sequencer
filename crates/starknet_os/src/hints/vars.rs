use std::collections::HashMap;

use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_types_core::felt::Felt;

pub(crate) enum Scope {
    InitialDict,
    DictManager,
    DictTracker,
    CommitmentInfoByAddress,
}

impl From<Scope> for &'static str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::InitialDict => "initial_dict",
            Scope::DictManager => "Dict_manager",
            Scope::DictTracker => "dict_tracker",
            Scope::CommitmentInfoByAddress => "commitment_info_by_address",
        }
    }
}

pub(crate) enum Ids {
    BucketIndex,
    ContractStateChanges,
    DictPtr,
    NextAvailableAlias,
    PrevOffset,
}

impl From<Ids> for &str {
    fn from(ids: Ids) -> &'static str {
        match ids {
            Ids::BucketIndex => "bucket_index",
            Ids::ContractStateChanges => "contract_state_changes",
            Ids::DictPtr => "dict_ptr",
            Ids::NextAvailableAlias => "next_available_alias",
            Ids::PrevOffset => "prev_offset",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Const {
    AliasContractAddress,
}

impl From<Const> for &'static str {
    fn from(constant: Const) -> &'static str {
        match constant {
            Const::AliasContractAddress => "ALIAS_CONTRACT_ADDRESS",
        }
    }
}

impl Const {
    pub fn fetch(&self, constants: &HashMap<String, Felt>) -> Result<Felt, HintError> {
        let identifier = (*self).into();
        constants.get(identifier).copied().ok_or(HintError::MissingConstant(Box::new(identifier)))
    }
}
