use std::any::Any;
use std::collections::HashMap;
use std::sync::LazyLock;

use ethnum::U256;
use num_bigint::{BigInt, BigUint, Sign};
use rand::Rng;
use rand_distr::num_traits::Num;
use starknet_os::hints::shared_utils::BASE;
use starknet_os::test_utils::cairo_runner::{EndpointArg, ImplicitArg};
use starknet_os::test_utils::utils::run_cairo_function_and_check_result;
use starknet_types_core::felt::Felt;

use crate::os_cli::tests::types::{OsPythonTestResult, OsSpecificTestError};
use crate::shared_utils::types::PythonTestError;

static DEFAULT_PRIME: LazyLock<BigUint> = LazyLock::new(|| {
    BigUint::from_str_radix(
        "3618502788666131213697322783095070105623107215331596699973092056135872020481",
        10,
    )
    .unwrap()
});

#[macro_export]
macro_rules! hashmap {
    ($( $key: expr => $value: expr ),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut map = HashMap::new();
        $(
            map.insert($key, $value);
        )*
        map
    }};
}

#[macro_export]
macro_rules! felt_to_felt_hashmap {
    ($( $key: expr => $value: expr ),* $(,)?) => {{
        hashmap! {
            $(
                starknet_types_core::felt::Felt::from($key) =>
                starknet_types_core::felt::Felt::from($value),
            )*
        }
    }};
}

#[macro_export]
macro_rules! felt_to_value_hashmap {
    ($( $key: expr => $value: expr ),* $(,)?) => {{
        hashmap! {
            $(
                starknet_types_core::felt::Felt::from($key) =>
                $value,
            )*
        }
    }};
}

#[macro_export]
macro_rules! felt_tuple {
    ($($value: expr),* $(,)?) => {
        (
            $(
                starknet_types_core::felt::Felt::from($value),
            )*
        )
    }
}

pub(crate) fn test_cairo_function(
    program_str: &str,
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_explicit_retdata: &[EndpointArg],
    expected_implicit_retdata: &[EndpointArg],
    hint_locals: HashMap<String, Box<dyn Any>>,
) -> OsPythonTestResult {
    run_cairo_function_and_check_result(
        program_str,
        function_name,
        explicit_args,
        implicit_args,
        expected_explicit_retdata,
        expected_implicit_retdata,
        hint_locals,
    )
    .map_err(|error| {
        PythonTestError::SpecificError(OsSpecificTestError::Cairo0EntryPointRunner(error))
    })?;
    Ok("".to_string())
}

// TODO(Amos): Consolidate with the equivalent function in `starknet_patricia`, once there is
// a utils crate.
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
        U256::from_words(rng.gen_range(*high_of_low..=*high_of_high), rng.gen_range(0..=u128::MAX))
    };
    let mut result = randomize();
    while result < low || result >= high {
        result = randomize();
    }
    result
}

// FIXME: Is this function needed?
/// Returns the lift of the given field element, val, as a `BigInt` in the range
/// (-prime/2, prime/2).
fn as_int(val: &Felt, prime: &BigInt) -> BigInt {
    let val = val.to_bigint();
    if val < (prime / BigInt::from(2)) {
        return val.clone();
    }
    val - prime
}

// FIXME: Is this function needed?
/// Takes a BigInt3 struct represented by the limbs (d0, d1, d2) of
/// and reconstructs the corresponding integer (see split_bigint3()).
/// Note that the limbs do not have to be in the range [0, BASE).
/// Prime is used to handle negative values of the limbs.
pub fn pack_bigint3(limbs: &[Felt; 3]) -> BigInt {
    let default_prime = BigInt::from_biguint(Sign::Plus, DEFAULT_PRIME.clone());
    limbs.iter().enumerate().fold(BigInt::ZERO, |acc, (i, &limb)| {
        acc + as_int(&limb, &default_prime) * BASE.pow(i.try_into().unwrap())
    })
}
