use starknet_api::core::CompiledClassHash;
use starknet_types_core::felt::Felt;

use crate::patricia_merkle_tree::node_data::errors::LeafResult;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;

impl Leaf for CompiledClassHash {
    type Input = Self;
    type Output = ();

    fn is_empty(&self) -> bool {
        self.0 == Felt::ZERO
    }

    async fn create(input: Self::Input) -> LeafResult<(Self, Self::Output)> {
        Ok((input, ()))
    }
}
