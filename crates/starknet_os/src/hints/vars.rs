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
            Scope::DictManager => "dict_manager",
            Scope::DictTracker => "dict_tracker",
            Scope::CommitmentInfoByAddress => "commitment_info_by_address",
        }
    }
}

pub(crate) enum Ids {
    BucketIndex,
    ContractStateChangesEnd,
    DictPtr,
    NextAvailableAlias,
    PrevOffset,
    AliasesEntry,
}

impl From<Ids> for &str {
    fn from(ids: Ids) -> &'static str {
        match ids {
            Ids::BucketIndex => "bucket_index",
            Ids::ContractStateChangesEnd => "contract_state_changes_end",
            Ids::DictPtr => "dict_ptr",
            Ids::NextAvailableAlias => "next_available_alias",
            Ids::PrevOffset => "prev_offset",
            Ids::AliasesEntry => "aliases_entry",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Const {
    AliasContractAddress,
    InitialAvailableAlias,
    AliasCounterStorageKey,
}

impl From<Const> for &'static str {
    fn from(constant: Const) -> &'static str {
        match constant {
            Const::AliasContractAddress => "ALIAS_CONTRACT_ADDRESS",
            Const::InitialAvailableAlias => "INITIAL_AVAILABLE_ALIAS",
            Const::AliasCounterStorageKey => "ALIAS_COUNTER_STORAGE_KEY",
        }
    }
}

impl Const {
    pub fn fetch<'a>(&self, constants: &'a HashMap<String, Felt>) -> Result<&'a Felt, HintError> {
        let identifier = (*self).into();
        constants.get(identifier).ok_or(HintError::MissingConstant(Box::new(identifier)))
    }
}
