use std::collections::HashMap;

use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData};
use starknet_types_core::felt::Felt;

#[derive(Clone)]
pub enum Preimage {
    Binary(BinaryData),
    Edge(EdgeData),
}

pub type PreimageMap = HashMap<Felt, Preimage>;

impl Preimage {
    pub fn length(&self) -> u8 {
        match self {
            Preimage::Binary(_) => 2,
            Preimage::Edge(_) => 3,
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum DecodeNodeCase {
    Left,
    Right,
    Both,
}
