use std::num::ParseIntError;
use std::path::Path;
use std::sync::LazyLock;

use ark_bls12_381::Fr;
use ark_ff::{BigInteger, PrimeField};
use ark_poly::{EvaluationDomain, Radix2EvaluationDomain};
use c_kzg::{Blob, KzgCommitment, KzgSettings, BYTES_PER_FIELD_ELEMENT};
use num_bigint::ParseBigIntError;
use num_traits::Zero;
use starknet_infra_utils::compile_time_cargo_manifest_dir;
use starknet_types_core::felt::Felt;

const COMMITMENT_BYTES_LENGTH: usize = 48;
const COMMITMENT_BYTES_MIDPOINT: usize = COMMITMENT_BYTES_LENGTH / 2;
const WIDTH: usize = 12;
pub(crate) const FIELD_ELEMENTS_PER_BLOB: usize = 1 << WIDTH;

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

pub(crate) fn split_commitment(commitment: &KzgCommitment) -> Result<(Felt, Felt), FftError> {
    let commitment_bytes: [u8; COMMITMENT_BYTES_LENGTH] = *commitment.to_bytes().as_ref();

    // Split the number.
    let low = &commitment_bytes[COMMITMENT_BYTES_MIDPOINT..];
    let high = &commitment_bytes[..COMMITMENT_BYTES_MIDPOINT];

    Ok((Felt::from_bytes_be_slice(low), Felt::from_bytes_be_slice(high)))
}

/// Performs bit-reversal permutation on the given vector, in-place.
/// Inlined from ark_poly.
// TODO(Dori): can we import this algorithm from somewhere?
pub(crate) fn bit_reversal<T>(unreversed_blob: &mut [T]) -> Result<(), FftError> {
    if unreversed_blob.len() != FIELD_ELEMENTS_PER_BLOB {
        return Err(FftError::InvalidBlobSize(unreversed_blob.len()));
    }

    fn bitreverse(mut n: usize) -> usize {
        let mut r = 0;
        for _ in 0..WIDTH {
            r = (r << 1) | (n & 1);
            n >>= 1;
        }
        r
    }

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
