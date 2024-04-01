use starknet_api::core::{ClassHash, Nonce};

use crate::patricia_merkle_tree::types::{LeafTrait, PathToBottom};
use crate::{hash::types::HashOutput, types::Felt};

#[allow(dead_code)]
pub(crate) enum FilledNode<L: LeafTrait> {
    Binary { data: BinaryData, hash: HashOutput },
    Edge { data: EdgeData, hash: HashOutput },
    Leaf(L),
}

#[allow(dead_code)]
pub(crate) struct BinaryData {
    left_hash: HashOutput,
    right_hash: HashOutput,
}

#[allow(dead_code)]
pub(crate) struct EdgeData {
    bottom_hash: HashOutput,
    path_to_bottom: PathToBottom,
}

#[allow(dead_code)]
pub(crate) enum LeafEnum {
    StorageValue(Felt),
    CompiledClassHash(Felt),
    StateTreeTuple {
        class_hash: ClassHash,
        contract_state_root_hash: Felt,
        nonce: Nonce,
    },
}
