use std::num::ParseIntError;
use std::sync::LazyLock;

use ark_bls12_381::Fr;
use ark_ff::{BigInteger, PrimeField};
use ark_poly::{EvaluationDomain, Radix2EvaluationDomain};
use c_kzg::{Blob, KzgCommitment, KzgSettings, BYTES_PER_FIELD_ELEMENT};
use num_bigint::{BigInt, BigUint, ParseBigIntError};
use num_traits::{Num, Signed, Zero};
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;

pub static BASE: LazyLock<BigInt> = LazyLock::new(|| BigInt::from(1u128 << 86));
pub static BLS_PRIME: LazyLock<BigUint> = LazyLock::new(|| {
    BigUint::from_str_radix(
        "52435875175126190479447740508185965837690552500527637822603658699938581184513",
        10,
    )
    .unwrap()
});

const TRUSTED_SETUP: &str = include_str!("trusted_setup.txt");
const COMMITMENT_BYTES_LENGTH: usize = 48;
const COMMITMENT_BYTES_MIDPOINT: usize = COMMITMENT_BYTES_LENGTH / 2;
const LOG2_FIELD_ELEMENTS_PER_BLOB: usize = 12;
pub(crate) const FIELD_ELEMENTS_PER_BLOB: usize = 1 << LOG2_FIELD_ELEMENTS_PER_BLOB;

#[derive(Debug, thiserror::Error)]
pub enum FftError {
    #[error(transparent)]
    CKzg(#[from] c_kzg::Error),
    #[error("Failed to create the evaluation domain.")]
    EvalDomainCreation,
    #[error(transparent)]
    InvalidBinaryToUsize(ParseIntError),
    #[error("Blob size must be {FIELD_ELEMENTS_PER_BLOB}, got {0}.")]
    InvalidBlobSize(usize),
    #[error("Raw blob size (in bytes) must be divisible by {BYTES_PER_FIELD_ELEMENT}, got {0}.")]
    InvalidRawBlobSize(usize),
    #[error(transparent)]
    ParseBigUint(#[from] ParseBigIntError),
    #[error("Too many coefficients; expected at most {FIELD_ELEMENTS_PER_BLOB}, got {0}.")]
    TooManyCoefficients(usize),
}

static KZG_SETTINGS: LazyLock<KzgSettings> = LazyLock::new(|| {
    KzgSettings::parse_kzg_trusted_setup(TRUSTED_SETUP)
        .unwrap_or_else(|error| panic!("Failed to load trusted setup: {error}."))
});

fn blob_to_kzg_commitment(blob: &Blob) -> Result<KzgCommitment, FftError> {
    Ok(KzgCommitment::blob_to_kzg_commitment(blob, &KZG_SETTINGS)?)
}

fn pad_bytes(input_bytes: Vec<u8>, length: usize) -> Vec<u8> {
    let mut bytes = input_bytes;
    let padding = length.saturating_sub(bytes.len());
    if padding > 0 {
        let mut padded_bytes = vec![0; padding];
        padded_bytes.extend(bytes);
        bytes = padded_bytes;
    }
    bytes
}

pub(crate) fn serialize_blob(blob: &[Fr]) -> Result<Vec<u8>, FftError> {
    if blob.len() != FIELD_ELEMENTS_PER_BLOB {
        return Err(FftError::InvalidBlobSize(blob.len()));
    }
    Ok(blob
        .iter()
        .flat_map(|x| pad_bytes(x.into_bigint().to_bytes_be(), BYTES_PER_FIELD_ELEMENT))
        .collect())
}

pub fn deserialize_blob(blob_bytes: &[u8]) -> Result<Vec<Fr>, FftError> {
    if blob_bytes.len() % BYTES_PER_FIELD_ELEMENT != 0 {
        return Err(FftError::InvalidRawBlobSize(blob_bytes.len()));
    }

    Ok(blob_bytes
        .chunks_exact(BYTES_PER_FIELD_ELEMENT)
        .map(|slice| Fr::from(BigUint::from_bytes_be(slice)))
        .collect())
}

pub(crate) fn split_commitment(commitment: &KzgCommitment) -> Result<(Felt, Felt), FftError> {
    let commitment_bytes: [u8; COMMITMENT_BYTES_LENGTH] = *commitment.to_bytes().as_ref();

    // Split the number.
    let low = &commitment_bytes[COMMITMENT_BYTES_MIDPOINT..];
    let high = &commitment_bytes[..COMMITMENT_BYTES_MIDPOINT];

    Ok((Felt::from_bytes_be_slice(low), Felt::from_bytes_be_slice(high)))
}

/// Performs bit-reversal permutation on the given vector, in-place.
/// Inlined from ark_poly.
// TODO(Dori): once the ark crates have a stable release with
//   [this change](https://github.com/arkworks-rs/algebra/pull/960) included, remove this function
//   and use `bitreverse_permutation_in_place`.
pub(crate) fn bit_reversal<T>(unreversed_blob: &mut [T]) -> Result<(), FftError> {
    if unreversed_blob.len() != FIELD_ELEMENTS_PER_BLOB {
        return Err(FftError::InvalidBlobSize(unreversed_blob.len()));
    }

    /// Reverses the bits of `n`, where `n` is represented by `LOG2_FIELD_ELEMENTS_PER_BLOB` bits.
    fn bitreverse(mut n: usize) -> usize {
        let mut r = 0;
        for _ in 0..LOG2_FIELD_ELEMENTS_PER_BLOB {
            // Mirror the bits: shift `n` right, shift `r` left.
            r = (r << 1) | (n & 1);
            n >>= 1;
        }
        r
    }

    // Applies the bit-reversal permutation on all elements. Swaps only when `i < reversed_i` to
    // avoid swapping the same element twice.
    for i in 0..FIELD_ELEMENTS_PER_BLOB {
        let reversed_i = bitreverse(i);
        if i < reversed_i {
            unreversed_blob.swap(i, reversed_i);
        }
    }

    Ok(())
}

pub(crate) fn polynomial_coefficients_to_blob(coefficients: Vec<Fr>) -> Result<Vec<u8>, FftError> {
    if coefficients.len() > FIELD_ELEMENTS_PER_BLOB {
        return Err(FftError::TooManyCoefficients(coefficients.len()));
    }

    // Pad with zeros to complete FIELD_ELEMENTS_PER_BLOB coefficients.
    let mut evals = coefficients;
    evals.resize(FIELD_ELEMENTS_PER_BLOB, Fr::zero());

    // Perform FFT (in place) on the coefficients, and bit-reverse.
    let domain = Radix2EvaluationDomain::<Fr>::new(FIELD_ELEMENTS_PER_BLOB)
        .ok_or(FftError::EvalDomainCreation)?;
    domain.fft_in_place(&mut evals);
    bit_reversal(&mut evals)?;

    // Serialize the FFT result into a blob.
    serialize_blob(&evals)
}

pub(crate) fn polynomial_coefficients_to_kzg_commitment(
    coefficients: Vec<Fr>,
) -> Result<(Felt, Felt), FftError> {
    let blob = polynomial_coefficients_to_blob(coefficients)?;
    let commitment_bytes = blob_to_kzg_commitment(&Blob::from_bytes(&blob)?)?;
    split_commitment(&commitment_bytes)
}

/// Takes an integer and returns its canonical representation as:
///    d0 + d1 * BASE + d2 * BASE**2.
/// d2 can be in the range (-2**127, 2**127).
// TODO(Dori): Consider using bls_split from the VM crate if and when public.
pub fn split_bigint3(num: BigInt) -> Result<[Felt; 3], OsHintError> {
    let (q1, d0) = (&num / &*BASE, Felt::from(num % &*BASE));
    let (d2, d1) = (&q1 / &*BASE, Felt::from(q1 % &*BASE));
    if d2.abs() >= BigInt::from(1_u128 << 127) {
        return Err(OsHintError::AssertionFailed {
            message: format!("Remainder should be in (-2**127, 2**127), got {d2}."),
        });
    }

    Ok([d0, d1, Felt::from(d2)])
}

pub(crate) fn horner_eval(coefficients: &[BigUint], point: &BigUint, prime: &BigUint) -> BigUint {
    coefficients.iter().rev().fold(BigUint::ZERO, |acc, coeff| (acc * point + coeff) % prime)
}
