use starknet_types_core::hash::{Poseidon, StarkHash};

use crate::hash::hash_trait::{HashFunction, HashInputPair, HashOutput};

pub struct PoseidonHashFunction;

impl HashFunction for PoseidonHashFunction {
    fn compute_hash(i: HashInputPair) -> HashOutput {
        HashOutput(Poseidon::hash(&i.0.into(), &i.1.into()).into())
    }
}
