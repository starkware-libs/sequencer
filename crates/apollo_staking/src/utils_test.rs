use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::StarkHash;

use crate::utils::{BlockPseudorandomGenerator, BlockRandomGenerator};

#[test]
fn test_deterministic_same_inputs() {
    let generator = BlockPseudorandomGenerator;
    let height = BlockNumber(100);
    let round = 5;
    let block_hash = Some(BlockHash(StarkHash::from(123u64)));
    let range = 10000;

    let result1 = generator.generate(height, round, block_hash, range);
    let result2 = generator.generate(height, round, block_hash, range);
    assert_eq!(result1, result2, "Same inputs should produce same output");
    assert!(result1 < range, "Result should be in range [0, {range}), but got {result1}");
}

#[test]
fn test_deterministic_none_block_hash() {
    let generator = BlockPseudorandomGenerator;
    let height = BlockNumber(100);
    let round = 5;
    let block_hash = None;
    let range = 10000;

    let result1 = generator.generate(height, round, block_hash, range);
    let result2 = generator.generate(height, round, block_hash, range);
    assert_eq!(result1, result2, "Same inputs with None block_hash should produce same output");
    assert!(result1 < range, "Result should be in range [0, {range}), but got {result1}");
}

#[test]
fn test_range_zero() {
    let generator = BlockPseudorandomGenerator;
    let height = BlockNumber(100);
    let round = 5;
    let block_hash = Some(BlockHash(StarkHash::from(123u64)));
    let range = 0;

    let result = generator.generate(height, round, block_hash, range);
    assert_eq!(result, 0, "Range 0 should return 0");
}
