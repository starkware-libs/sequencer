use std::error::Error;

use starknet_api::transaction::fields::Proof;
use starknet_types_core::felt::Felt;

pub trait ProofStorage: Send + Sync {
    type Error: Error;
    fn set_proof(&self, facts_hash: Felt, proof: Proof) -> Result<(), Self::Error>;
    fn get_proof(&self, facts_hash: Felt) -> Result<Option<Proof>, Self::Error>;
    fn contains_proof(&self, facts_hash: Felt) -> Result<bool, Self::Error>;
}
