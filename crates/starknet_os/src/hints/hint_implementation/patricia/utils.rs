use std::collections::HashMap;

use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData};

use crate::hints::hint_implementation::patricia::error::PatriciaError;

#[derive(Clone)]
pub enum Preimage {
    Binary(BinaryData),
    Edge(EdgeData),
}

impl Preimage {
    pub fn length(&self) -> u8 {
        match self {
            Preimage::Binary(_) => 2,
            Preimage::Edge(_) => 3,
        }
    }

    fn get_binary(&self) -> Result<&BinaryData, PatriciaError> {
        match self {
            Preimage::Binary(binary) => Ok(binary),
            _ => Err(PatriciaError::ExpectedBinary),
        }
    }
}

pub type PreimageMap = HashMap<HashOutput, Preimage>;

#[derive(Clone, PartialEq)]
pub enum DecodeNodeCase {
    Left,
    Right,
    Both,
}
