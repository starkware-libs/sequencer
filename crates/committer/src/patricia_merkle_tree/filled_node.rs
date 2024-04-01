use starknet_api::core::{ClassHash, Nonce};

use crate::patricia_merkle_tree::types::{LeafDataTrait, PathToBottom};
use crate::{hash::types::HashOutput, types::Felt};

#[allow(dead_code)]
pub(crate) struct FilledNode<L: LeafDataTrait> {
    hash: HashOutput,
    data: NodeData<L>,
}

#[allow(dead_code)]
pub(crate) enum NodeData<L: LeafDataTrait> {
    Binary(BinaryData),
    Edge(EdgeData),
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
pub(crate) enum LeafData {
    StorageValue(Felt),
    CompiledClassHash(Felt),
    StateTreeTuple {
        class_hash: ClassHash,
        contract_state_root_hash: Felt,
        nonce: Nonce,
    },
}
