use starknet_types_core::hash::{Pedersen, StarkHash};

use crate::hash::hash_trait::{HashFunction, HashInputPair, HashOutput};

pub struct PedersenHashFunction;

impl HashFunction for PedersenHashFunction {
    fn compute_hash(i: HashInputPair) -> HashOutput {
        HashOutput(Pedersen::hash(&i.0.into(), &i.1.into()).into())
    }
}
