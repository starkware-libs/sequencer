use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::{ClassHash, Nonce};

pub trait LeafData {
    /// Returns true if leaf is empty.
    fn is_empty(&self) -> bool;
}
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractState {
    pub nonce: Nonce,
    pub storage_root_hash: HashOutput,
    pub class_hash: ClassHash,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeafDataImpl {
    StorageValue(Felt),
    CompiledClassHash(ClassHash),
    ContractState(ContractState),
}

impl LeafData for LeafDataImpl {
    fn is_empty(&self) -> bool {
        match self {
            LeafDataImpl::StorageValue(value) => *value == Felt::ZERO,
            LeafDataImpl::CompiledClassHash(class_hash) => class_hash.0 == Felt::ZERO,
            LeafDataImpl::ContractState(contract_state) => {
                contract_state.nonce.0 == Felt::ZERO
                    && contract_state.class_hash.0 == Felt::ZERO
                    && contract_state.storage_root_hash.0 == Felt::ZERO
            }
        }
    }
}
