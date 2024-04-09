use starknet_types_core::hash::{Pedersen, StarkHash};

use crate::types::Felt;

#[allow(dead_code)]
pub(crate) struct HashInputPair(pub Felt, pub Felt);

#[allow(dead_code)]
pub(crate) struct HashOutput(pub Felt);

#[allow(dead_code)]
impl HashOutput {
    pub(crate) const ZERO: HashOutput = HashOutput(Felt::ZERO);
}

pub(crate) trait HashFunction {
    /// Computes the hash of given input.
    fn compute_hash(i: HashInputPair) -> HashOutput;
}

pub(crate) struct PedersenHashFunction;

impl HashFunction for PedersenHashFunction {
    fn compute_hash(i: HashInputPair) -> HashOutput {
        HashOutput(Pedersen::hash(&i.0, &i.1))
    }
}
