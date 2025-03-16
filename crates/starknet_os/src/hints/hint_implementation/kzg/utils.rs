use std::num::ParseIntError;
use std::path::Path;
use std::sync::LazyLock;

use blockifier::utils::usize_from_u32;
use c_kzg::{Blob, KzgCommitment, KzgSettings, BYTES_PER_FIELD_ELEMENT};
use num_bigint::{BigUint, ParseBigIntError};
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
    ParseBigUint(#[from] ParseBigIntError),
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

fn to_bytes(x: &BigUint, length: usize) -> Vec<u8> {
    let mut bytes = x.to_bytes_be();
    let padding = length.saturating_sub(bytes.len());
    if padding > 0 {
        let mut padded_bytes = vec![0; padding];
        padded_bytes.extend(bytes);
        bytes = padded_bytes;
    }
    bytes
}

fn serialize_blob(blob: &[BigUint]) -> Result<Vec<u8>, FftError> {
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
/// * `coeffs` - A slice of `BigUint` representing the coefficients of the polynomial.
/// * `group` - A slice of `BigUint` representing the precomputed group elements for the FFT.
/// * `prime` - A `BigUint` representing the prime modulus for the field operations.
///
/// # Returns
///
/// A `Vec<BigUint>` containing the transformed coefficients after applying the FFT.
///
/// # See More
/// - <https://en.wikipedia.org/wiki/Fast_Fourier_transform>
/// - <https://github.com/starkware-libs/cairo-lang/blob/v0.13.2/src/starkware/python/math_utils.py#L310>
fn inner_fft(coeffs: &[BigUint], group: &[BigUint], prime: &BigUint) -> Vec<BigUint> {
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

    let group_mul_f_odd: Vec<BigUint> =
        group.iter().take(f_odd.len()).zip(f_odd.iter()).map(|(g, f)| (g * f) % prime).collect();

    let mut result = Vec::with_capacity(coeffs.len());
    for i in 0..f_even.len() {
        result.push((f_even[i].clone() + &group_mul_f_odd[i]) % prime);
    }
    for i in 0..f_even.len() {
        // Ensure non-negative diff by adding prime to the value before subtracting.
        let diff = ((f_even[i].clone() + prime) - &group_mul_f_odd[i]) % prime;
        result.push(diff);
    }

    result
}

/// Computes the FFT of `coeffs`, assuming the size of the coefficient array is a power of two and
/// equals to the generator's multiplicative order.
///
/// See more: <https://github.com/starkware-libs/cairo-lang/blob/v0.13.2/src/starkware/python/math_utils.py#L304>
pub(crate) fn fft(
    coeffs: &[BigUint],
    generator: &BigUint,
    prime: &BigUint,
    bit_reversed: bool,
) -> Result<Vec<BigUint>, FftError> {
    if coeffs.is_empty() {
        return Ok(vec![]);
    }

    let coeffs_len = coeffs.len();
    if !coeffs_len.is_power_of_two() {
        return Err(FftError::InvalidCoeffsLength(coeffs_len));
    }

    let mut group = vec![BigUint::one()];
    for _ in 0..(coeffs_len - 1) {
        let last = group.last().expect("Group is never empty.");
        group.push((last * generator) % prime);
    }

    let mut values = inner_fft(coeffs, &group, prime);

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

fn polynomial_coefficients_to_blob(coefficients: Vec<BigUint>) -> Result<Vec<u8>, FftError> {
    if coefficients.len() > FIELD_ELEMENTS_PER_BLOB {
        return Err(FftError::TooManyCoefficients(coefficients.len()));
    }

    // Pad with zeros to complete FIELD_ELEMENTS_PER_BLOB coefficients.
    let mut padded_coefficients = coefficients;
    padded_coefficients.resize(FIELD_ELEMENTS_PER_BLOB, BigUint::zero());

    // Perform FFT on the coefficients
    let generator = BigUint::from_str_radix(BLOB_SUBGROUP_GENERATOR, 10)?;
    let prime = BigUint::from_str_radix(BLS_PRIME, 10)?;
    let bit_reversed = true;
    let fft_result = fft(&padded_coefficients, &generator, &prime, bit_reversed)?;

    // Serialize the FFT result into a blob.
    serialize_blob(&fft_result)
}

pub(crate) fn polynomial_coefficients_to_kzg_commitment(
    coefficients: Vec<BigUint>,
) -> Result<(Felt, Felt), FftError> {
    let blob = polynomial_coefficients_to_blob(coefficients)?;
    let commitment_bytes = blob_to_kzg_commitment(&Blob::from_bytes(&blob)?)?;
    split_commitment(&commitment_bytes)
}
