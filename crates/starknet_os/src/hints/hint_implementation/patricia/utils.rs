use std::collections::HashMap;

use starknet_types_core::felt::Felt;

pub type Preimage = HashMap<Felt, Vec<Felt>>;

#[derive(Clone, PartialEq)]
pub enum DecodeNodeCase {
    Left,
    Right,
    Both,
}
