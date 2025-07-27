use apollo_consensus::types::Round;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::{BlockHash, BlockNumber};

#[cfg_attr(test, automock)]
pub trait BlockRandomGenerator: Send + Sync {
    fn generate(
        &self,
        height: BlockNumber,
        round: Round,
        block_hash: Option<BlockHash>,
        range: u128,
    ) -> u128;
}

#[allow(dead_code)]
pub struct BlockPseudorandomGenerator;

impl BlockRandomGenerator for BlockPseudorandomGenerator {
    fn generate(
        &self,
        _height: BlockNumber,
        _round: Round,
        _block_hash: Option<BlockHash>,
        _range: u128,
    ) -> u128 {
        todo!()
    }
}
