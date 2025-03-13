use std::num::ParseIntError;
use std::path::Path;
use std::slice::Iter;
use std::sync::LazyLock;

use blockifier::utils::usize_from_u32;
use c_kzg::{Blob, KzgCommitment, KzgSettings, BYTES_PER_FIELD_ELEMENT};
use num_bigint::{BigInt, ParseBigIntError};
use num_traits::{Num, One, Zero};
use starknet_infra_utils::compile_time_cargo_manifest_dir;
use starknet_types_core::felt::Felt;

const BLOB_SUBGROUP_GENERATOR: &str =
    "39033254847818212395286706435128746857159659164139250548781411570340225835782";
pub(crate) const BLS_PRIME: &str =
    "52435875175126190479447740508185965837690552500527637822603658699938581184513";
const COMMITMENT_BYTES_LENGTH: usize = 48;
const COMMITMENT_BYTES_MIDPOINT: usize = COMMITMENT_BYTES_LENGTH / 2;
const FIELD_ELEMENTS_PER_BLOB: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum FftError {
    #[error(transparent)]
    CKzg(#[from] c_kzg::Error),
    #[error(transparent)]
    InvalidBinaryToUsize(ParseIntError),
    #[error("Blob size must be {FIELD_ELEMENTS_PER_BLOB}, got {0}.")]
    InvalidBlobSize(usize),
    #[error("Invalid coefficients length (must be a power of two): {0}.")]
    InvalidCoeffsLength(usize),
    #[error(transparent)]
    ParseBigInt(#[from] ParseBigIntError),
    #[error("Too many coefficients; expected at most {FIELD_ELEMENTS_PER_BLOB}, got {0}.")]
    TooManyCoefficients(usize),
}

static KZG_SETTINGS: LazyLock<KzgSettings> = LazyLock::new(|| {
    let path =
        Path::new(compile_time_cargo_manifest_dir!()).join("resources").join("trusted_setup.txt");
    KzgSettings::load_trusted_setup_file(&path)
        .unwrap_or_else(|error| panic!("Failed to load trusted setup file from {path:?}: {error}."))
});

fn blob_to_kzg_commitment(blob: &Blob) -> Result<KzgCommitment, FftError> {
    Ok(KzgCommitment::blob_to_kzg_commitment(blob, &KZG_SETTINGS)?)
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
/// * `coeffs` - A slice of `BigInt` representing the coefficients of the polynomial. Note that this
///   iterator is an iterator over all coefficients of the original (outermost) call; the actual
///   coefficients of the current layer can be deduced by the `step_interval` and `coeffs_skip`
///   parameters.
/// * `group` - A slice of `BigInt` representing the precomputed group elements for the FFT.
/// * `prime` - A `BigInt` representing the prime modulus for the field operations.
/// * `step_interval` - The interval between elements of the original coefficients relevant in the
///   current layer. Should be `2.pow(layer)` where `layer` is the FFT depth (starting at zero).
/// * `coeffs_skip` - The number of coefficients to skip in the original coefficients. Used to
///   compute the "odd" slice of coefficients in the current layer. Note that this is a flat amount,
///   so the `step_interval` must be taken into account when recursing (see comment in the recursive
///   call).
///
/// # Returns
///
/// A `Vec<BigInt>` containing the transformed coefficients after applying the FFT.
///
/// # See More
/// - <https://en.wikipedia.org/wiki/Fast_Fourier_transform>
/// - <https://github.com/starkware-libs/cairo-lang/blob/v0.13.2/src/starkware/python/math_utils.py#L310>
fn inner_fft(
    coeffs: Iter<'_, BigInt>,
    group: Iter<'_, BigInt>,
    prime: &BigInt,
    step_interval: usize,
    coeffs_skip: usize,
) -> Vec<BigInt> {
    let coeffs_len = coeffs.clone().skip(coeffs_skip).step_by(step_interval).len();
    if coeffs_len == 1 {
        return coeffs.skip(coeffs_skip).step_by(step_interval).cloned().collect();
    }

    let f_even = inner_fft(coeffs.clone(), group.clone(), prime, step_interval * 2, coeffs_skip);
    let f_odd = inner_fft(
        coeffs.clone(),
        group.clone(),
        prime,
        step_interval * 2,
        // The `skip` is applied before the `step_by`, so to correctly skip the first coefficient
        // in the layer in question, one must take the interval into account.
        // For example, if the original coefficients are [5, 6, 7, 8] and we are in layer 1 (step
        // is 2), then the coefficients of the current layer are [5, 7]. The odd coefficient is 7,
        // so we want to skip 5 and reach 7 (add 2 to the skip value).
        coeffs_skip + step_interval,
    );

    let group_mul_f_odd: Vec<BigInt> = group
        .step_by(step_interval)
        .take(f_odd.len())
        .zip(f_odd.iter())
        .map(|(g, f)| (g * f) % prime)
        .collect();

    let mut result = Vec::with_capacity(coeffs_len);
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

    let mut values = inner_fft(coeffs.iter(), group.iter(), prime, 1, 0);

    // TODO(Dori): either remove the custom FFT implementation entirely, or investigate implementing
    //   the bit-reversal permutation more efficiently.
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

pub(crate) fn split_commitment(commitment: &KzgCommitment) -> Result<(Felt, Felt), FftError> {
    let commitment_bytes: [u8; COMMITMENT_BYTES_LENGTH] = *commitment.to_bytes().as_ref();

    // Split the number.
    let low = &commitment_bytes[COMMITMENT_BYTES_MIDPOINT..];
    let high = &commitment_bytes[..COMMITMENT_BYTES_MIDPOINT];

    Ok((Felt::from_bytes_be_slice(low), Felt::from_bytes_be_slice(high)))
}

fn polynomial_coefficients_to_blob(coefficients: Vec<BigInt>) -> Result<Vec<u8>, FftError> {
    if coefficients.len() > FIELD_ELEMENTS_PER_BLOB {
        return Err(FftError::TooManyCoefficients(coefficients.len()));
    }

    // Pad with zeros to complete FIELD_ELEMENTS_PER_BLOB coefficients.
    let mut padded_coefficients = coefficients;
    padded_coefficients.resize(FIELD_ELEMENTS_PER_BLOB, BigInt::zero());

    // Perform FFT on the coefficients
    let generator = BigInt::from_str_radix(BLOB_SUBGROUP_GENERATOR, 10)?;
    let prime = BigInt::from_str_radix(BLS_PRIME, 10)?;
    let bit_reversed = true;
    let fft_result = fft(&padded_coefficients, &generator, &prime, bit_reversed)?;

    // Serialize the FFT result into a blob.
    serialize_blob(&fft_result)
}

pub(crate) fn polynomial_coefficients_to_kzg_commitment(
    coefficients: Vec<BigInt>,
) -> Result<(Felt, Felt), FftError> {
    let blob = polynomial_coefficients_to_blob(coefficients)?;
    let commitment_bytes = blob_to_kzg_commitment(&Blob::from_bytes(&blob)?)?;
    split_commitment(&commitment_bytes)
}
