use std::num::ParseIntError;

use blockifier::utils::usize_from_u32;
use c_kzg::BYTES_PER_FIELD_ELEMENT;
use num_bigint::BigInt;
use num_traits::{Num, One, Zero};

const BLOB_SUBGROUP_GENERATOR: &str =
    "39033254847818212395286706435128746857159659164139250548781411570340225835782";
#[allow(dead_code)]
pub(crate) const BLS_PRIME: &str =
    "52435875175126190479447740508185965837690552500527637822603658699938581184513";
const FIELD_ELEMENTS_PER_BLOB: usize = 4096;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum FftError {
    #[error(transparent)]
    InvalidBinaryToUsize(ParseIntError),
    #[error("Blob size must be {FIELD_ELEMENTS_PER_BLOB}, got {0}.")]
    InvalidBlobSize(usize),
    #[error("Invalid coefficients length (must be a power of two): {0}.")]
    InvalidCoeffsLength(usize),
    #[error("Could not parse BigInt: {0}")]
    ParseBigIntError(ParseBigIntError),
    #[error("Too many coefficients")]
    TooManyCoefficients,
}

fn to_bytes(x: &BigInt, length: usize) -> Vec<u8> {
    let mut bytes = x.to_bytes_be().1;
    let padding = length.saturating_sub(bytes.len());
    if padding > 0 {
        let mut padded_bytes = vec![0; padding];
        padded_bytes.extend(bytes);
        bytes = padded_bytes;
    }
    bytes
}

#[allow(dead_code)]
fn serialize_blob(blob: &[BigInt]) -> Result<Vec<u8>, FftError> {
    if blob.len() != FIELD_ELEMENTS_PER_BLOB {
        return Err(FftError::InvalidBlobSize(blob.len()));
    }
    Ok(blob.iter().flat_map(|x| to_bytes(x, BYTES_PER_FIELD_ELEMENT)).collect())
}

/// Performs the recursive Fast Fourier Transform (FFT) on the input coefficient vector `coeffs`
/// using the provided group elements `group` and modulus `prime`.
///
/// # Arguments
///
/// * `coeffs` - A slice of `BigInt` representing the coefficients of the polynomial.
/// * `group` - A slice of `BigInt` representing the precomputed group elements for the FFT.
/// * `prime` - A `BigInt` representing the prime modulus for the field operations.
///
/// # Returns
///
/// A `Vec<BigInt>` containing the transformed coefficients after applying the FFT.
///
/// # See More
/// - <https://en.wikipedia.org/wiki/Fast_Fourier_transform>
/// - <https://github.com/starkware-libs/cairo-lang/blob/v0.13.2/src/starkware/python/math_utils.py#L310>
fn inner_fft(coeffs: &[BigInt], group: &[BigInt], prime: &BigInt) -> Vec<BigInt> {
    if coeffs.len() == 1 {
        return coeffs.to_vec();
    }

    // TODO(Dori): Try to avoid the clones here (possibly by using a non-recursive implementation).
    let f_even = inner_fft(
        &coeffs.iter().step_by(2).cloned().collect::<Vec<_>>(),
        &group.iter().step_by(2).cloned().collect::<Vec<_>>(),
        prime,
    );
    let f_odd = inner_fft(
        &coeffs.iter().skip(1).step_by(2).cloned().collect::<Vec<_>>(),
        &group.iter().step_by(2).cloned().collect::<Vec<_>>(),
        prime,
    );

    let group_mul_f_odd: Vec<BigInt> =
        group.iter().take(f_odd.len()).zip(f_odd.iter()).map(|(g, f)| (g * f) % prime).collect();

    let mut result = Vec::with_capacity(coeffs.len());
    for i in 0..f_even.len() {
        result.push((f_even[i].clone() + &group_mul_f_odd[i]) % prime);
    }
    for i in 0..f_even.len() {
        // Ensure non-negative diff by adding prime to the value before applying the modulo.
        let diff = (f_even[i].clone() - &group_mul_f_odd[i] + prime) % prime;
        result.push(diff);
    }

    result
}

/// Computes the FFT of `coeffs`, assuming the size of the coefficient array is a power of two and
/// equals to the generator's multiplicative order.
///
/// See more: <https://github.com/starkware-libs/cairo-lang/blob/v0.13.2/src/starkware/python/math_utils.py#L304>
#[allow(dead_code)]
pub(crate) fn fft(
    coeffs: &[BigInt],
    generator: &BigInt,
    prime: &BigInt,
    bit_reversed: bool,
) -> Result<Vec<BigInt>, FftError> {
    if coeffs.is_empty() {
        return Ok(vec![]);
    }

    let coeffs_len = coeffs.len();
    if !coeffs_len.is_power_of_two() {
        return Err(FftError::InvalidCoeffsLength(coeffs_len));
    }

    let mut group = vec![BigInt::one()];
    for _ in 0..(coeffs_len - 1) {
        let last = group.last().expect("Group is never empty.");
        group.push((last * generator) % prime);
    }

    let mut values = inner_fft(coeffs, &group, prime);

    if bit_reversed {
        // Since coeffs_len is a power of two, width is set to the position of the last set bit.
        let width = usize_from_u32(coeffs_len.trailing_zeros());
        let perm = (0..coeffs_len)
            .map(|i| {
                let binary = format!("{:0width$b}", i, width = width);
                usize::from_str_radix(&binary.chars().rev().collect::<String>(), 2)
                    .map_err(FftError::InvalidBinaryToUsize)
            })
            .collect::<Result<Vec<_>, _>>()?;
        values = perm.into_iter().map(|i| values[i].clone()).collect();
    }

    Ok(values)
}

fn polynomial_coefficients_to_blob(coefficients: Vec<BigInt>) -> Result<Vec<u8>, FftError> {
    if coefficients.len() > FIELD_ELEMENTS_PER_BLOB {
        return Err(FftError::TooManyCoefficients);
    }

    // Pad with zeros to complete FIELD_ELEMENTS_PER_BLOB coefficients
    let mut padded_coefficients = coefficients;
    padded_coefficients.resize(FIELD_ELEMENTS_PER_BLOB, BigInt::zero());

    // Perform FFT on the coefficients
    let generator =
        BigInt::from_str_radix(BLOB_SUBGROUP_GENERATOR, 10).map_err(FftError::ParseBigIntError)?;
    let prime = BigInt::from_str_radix(BLS_PRIME, 10).map_err(FftError::ParseBigIntError)?;
    let fft_result = fft(&padded_coefficients, &generator, &prime, true)?;

    // Serialize the FFT result into a blob
    Ok(serialize_blob(&fft_result))
}
