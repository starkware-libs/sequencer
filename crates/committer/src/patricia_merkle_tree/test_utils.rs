use ethnum::U256;
use rand::Rng;
use rstest::rstest;

use crate::felt::Felt;
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePath, EdgePathLength, PathToBottom};

impl From<&str> for PathToBottom {
    fn from(value: &str) -> Self {
        Self {
            path: EdgePath(Felt::from(
                u128::from_str_radix(value, 2).expect("Invalid binary string"),
            )),
            length: EdgePathLength(
                (value.len() - if value.starts_with('+') { 1 } else { 0 })
                    .try_into()
                    .expect("String is too large"),
            ),
        }
    }
}
/// Generates a random U256 number between low and high (exclusive).
/// Panics if low > high.
pub(crate) fn get_random_u256(low: U256, high: U256) -> U256 {
    assert!(low < high);
    let high_of_low = low.high();
    let high_of_high = high.high();

    let delta = high - low;
    if delta <= u128::MAX {
        let delta = u128::try_from(delta).unwrap();
        return low + rand::thread_rng().gen_range(0..delta);
    }

    // Randomize the high 128 bits in the extracted range, and the low 128 bits in their entire
    // domain until the result is in range.
    // As high-low>u128::MAX, the expected number of samples until the loops breaks is bound from
    // above by 3 (as either:
    //  1. high_of_high > high_of_low + 1, and there is a 1/3 chance to get a valid result for high
    //  bits in (high_of_low, high_of_high).
    //  2. high_of_high == high_of_low + 1, and every possible low 128 bits value is valid either
    // when the high bits equal high_of_high, or when they equal high_of_low).
    let randomize = || {
        U256::from_words(
            rand::thread_rng().gen_range(*high_of_low..=*high_of_high),
            rand::thread_rng().gen_range(0..=u128::MAX),
        )
    };
    let mut result = randomize();
    while result < low || result >= high {
        result = randomize();
    }
    result
}

#[rstest]
#[should_panic]
#[case(U256::ZERO, U256::ZERO)]
#[case(U256::ZERO, U256::ONE)]
#[case(U256::ONE, U256::ONE << 128)]
#[case((U256::ONE<<128)-U256::ONE, U256::ONE << 128)]
#[case(U256::ONE<<128, (U256::ONE << 128)+U256::ONE)]
fn test_get_random_u256(#[case] low: U256, #[case] high: U256) {
    let r = get_random_u256(low, high);
    assert!(low <= r && r < high);
}
