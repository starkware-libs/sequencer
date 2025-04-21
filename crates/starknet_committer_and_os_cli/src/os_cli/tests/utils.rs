use std::any::Any;
use std::collections::HashMap;
use std::sync::LazyLock;

use ethnum::U256;
use num_bigint::{BigInt, Sign};
use rand::Rng;
use starknet_os::test_utils::cairo_runner::{EndpointArg, EntryPointRunnerConfig, ImplicitArg};
use starknet_os::test_utils::utils::run_cairo_function_and_check_result;

use crate::os_cli::tests::types::{OsPythonTestResult, OsSpecificTestError};
use crate::shared_utils::types::PythonTestError;

// 2**251 + 17 * 2**192 + 1
pub static DEFAULT_PRIME: LazyLock<BigInt> = LazyLock::new(|| {
    BigInt::from_bytes_be(
        Sign::Plus,
        &(U256::from(2_u32).pow(251) + 17 * U256::from(2_u32).pow(192) + 1).to_be_bytes(),
    )
});

#[allow(clippy::too_many_arguments)]
pub(crate) fn test_cairo_function(
    runner_config: &EntryPointRunnerConfig,
    program_str: &str,
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_explicit_retdata: &[EndpointArg],
    expected_implicit_retdata: &[EndpointArg],
    hint_locals: HashMap<String, Box<dyn Any>>,
) -> OsPythonTestResult {
    run_cairo_function_and_check_result(
        runner_config,
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

/// Generates a random U256 number between low and high (inclusive).
/// Panics if low > high
pub fn get_random_u256_inclusive<R: Rng>(rng: &mut R, low: U256, high: U256) -> U256 {
    assert!(low <= high, "low must be less than or equal to high. actual: {low} > {high}");

    let delta = high - low;
    if delta <= u128::MAX {
        let delta = u128::try_from(delta).expect("Failed to convert delta to u128");
        return low + rng.gen_range(0..=delta);
    }

    let low_of_low = low.low();
    let high_of_low = low.high();
    let low_of_high = high.low();
    let high_of_high = high.high();

    let random_high = rng.gen_range(*high_of_low..=*high_of_high);

    let random_low = if random_high == *high_of_low {
        rng.gen_range(*low_of_low..=u128::MAX)
    // Since high - low > u128::MAX, high_of_low != high_of_high.
    } else if random_high == *high_of_high {
        rng.gen_range(0..=*low_of_high)
    } else {
        rng.gen_range(0..=u128::MAX)
    };
    U256::from_words(random_high, random_low)
}
