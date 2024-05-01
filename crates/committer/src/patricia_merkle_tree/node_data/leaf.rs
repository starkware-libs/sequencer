use crate::felt::Felt;
use crate::patricia_merkle_tree::filled_tree::node::{ClassHash, Nonce};

pub trait LeafData {
    /// Returns true if leaf is empty.
    fn is_empty(&self) -> bool;
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeafDataImpl {
    StorageValue(Felt),
    CompiledClassHash(ClassHash),
    StateTreeTuple {
        class_hash: ClassHash,
        contract_state_root_hash: Felt,
        nonce: Nonce,
    },
}

impl LeafData for LeafDataImpl {
    fn is_empty(&self) -> bool {
        match self {
            LeafDataImpl::StorageValue(value) => *value == Felt::ZERO,
            LeafDataImpl::CompiledClassHash(class_hash) => class_hash.0 == Felt::ZERO,
            LeafDataImpl::StateTreeTuple {
                class_hash,
                contract_state_root_hash,
                nonce,
            } => {
                nonce.0 == Felt::ZERO
                    && class_hash.0 == Felt::ZERO
                    && *contract_state_root_hash == Felt::ZERO
            }
        }
    }
}
