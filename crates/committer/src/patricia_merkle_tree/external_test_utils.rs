use ethnum::U256;

use crate::felt::Felt;
use crate::patricia_merkle_tree::errors::TypesError;
use rand::Rng;

impl TryFrom<&U256> for Felt {
    type Error = TypesError<U256>;
    fn try_from(value: &U256) -> Result<Self, Self::Error> {
        if *value > U256::from(&Felt::MAX) {
            return Err(TypesError::ConversionError {
                from: *value,
                to: "Felt",
                reason: "value is bigger than felt::max",
            });
        }
        Ok(Self::from_bytes_be(&value.to_be_bytes()))
    }
}

/// Generates a random U256 number between low and high (exclusive).
/// Panics if low > high
pub fn get_random_u256<R: Rng>(rng: &mut R, low: U256, high: U256) -> U256 {
    assert!(low < high);
    let high_of_low = low.high();
    let high_of_high = high.high();

    let delta = high - low;
    if delta <= u128::MAX {
        let delta = u128::try_from(delta).expect("Failed to convert delta to u128");
        return low + rng.gen_range(0..delta);
    }

    // Randomize the high 128 bits in the extracted range, and the low 128 bits in their entire
    // domain until the result is in range.
    // As high-low>u128::MAX, the expected number of samples until the loops breaks is bound from
    // above by 3 (as either:
    //  1. high_of_high > high_of_low + 1, and there is a 1/3 chance to get a valid result for high
    //  bits in (high_of_low, high_of_high).
    //  2. high_of_high == high_of_low + 1, and every possible low 128 bits value is valid either
    // when the high bits equal high_of_high, or when they equal high_of_low).
    let mut randomize = || {
        U256::from_words(
            rng.gen_range(*high_of_low..=*high_of_high),
            rng.gen_range(0..=u128::MAX),
        )
    };
    let mut result = randomize();
    while result < low || result >= high {
        result = randomize();
    }
    result
}
