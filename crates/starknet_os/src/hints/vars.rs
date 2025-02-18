use std::collections::HashMap;

use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_types_core::felt::Felt;

pub(crate) enum Scope {
    InitialDict,
    DictTracker,
}

impl From<Scope> for &'static str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::InitialDict => "initial_dict",
            Scope::DictTracker => "dict_tracker",
        }
    }
}

pub(crate) enum Ids {
    BucketIndex,
    DictPtr,
    PrevOffset,
}

impl From<Ids> for &str {
    fn from(ids: Ids) -> &'static str {
        match ids {
            Ids::DictPtr => "dict_ptr",
            Ids::BucketIndex => "bucket_index",
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
