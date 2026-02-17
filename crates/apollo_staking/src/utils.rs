use apollo_protobuf::consensus::Round;
#[cfg(test)]
use mockall::automock;
use sha2::{Digest, Sha256};
use starknet_api::block::{BlockHash, BlockNumber};

#[cfg(test)]
#[path = "utils_test.rs"]
mod utils_test;

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
        height: BlockNumber,
        round: Round,
        block_hash: Option<BlockHash>,
        range: u128,
    ) -> u128 {
        if range == 0 {
            return 0;
        }
        let mut hasher = Sha256::new();

        hasher.update(height.0.to_be_bytes());
        hasher.update(round.to_be_bytes());
        if let Some(hash) = block_hash {
            hasher.update(hash.0.to_bytes_be().as_slice());
        } else {
            hasher.update([0u8; 32]);
        }

        let hash_bytes = hasher.finalize();

        // Since SHA256 is fixed 32 bytes, grab the last 16 bytes to extract a u128.
        let hash_value = u128::from_be_bytes(
            hash_bytes[16..32].try_into().expect("Failed to convert hash bytes to u128"),
        );
        // Return value in range [0, range)
        hash_value % range
    }
}
