use std::num::ParseIntError;
use std::sync::LazyLock;

use ark_bls12_381::Fr;
use ark_ff::{BigInteger, PrimeField};
use ark_poly::{EvaluationDomain, Radix2EvaluationDomain};
use c_kzg::{Blob, KzgCommitment, KzgSettings, BYTES_PER_BLOB, BYTES_PER_FIELD_ELEMENT};
use num_bigint::{BigInt, BigUint, ParseBigIntError};
use num_traits::{Num, Signed, Zero};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};
use sha2::{Digest, Sha256};
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
const _: () = assert!(BYTES_PER_BLOB == FIELD_ELEMENTS_PER_BLOB * BYTES_PER_FIELD_ELEMENT);

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
    #[error("Invalid bytes in blob: {0}.")]
    InvalidBytesInBlob(usize),
    #[error(transparent)]
    ParseBigUint(#[from] ParseBigIntError),
    #[error("Too many coefficients; expected at most {FIELD_ELEMENTS_PER_BLOB}, got {0}.")]
    TooManyCoefficients(usize),
}

static KZG_SETTINGS: LazyLock<KzgSettings> = LazyLock::new(|| {
    KzgSettings::parse_kzg_trusted_setup(TRUSTED_SETUP, 0)
        .unwrap_or_else(|error| panic!("Failed to load trusted setup: {error}."))
});

fn blob_to_kzg_commitment(blob: &Blob) -> Result<KzgCommitment, FftError> {
    Ok(KZG_SETTINGS.blob_to_kzg_commitment(blob)?)
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

pub(crate) fn serialize_blob(blob: &[Fr]) -> Result<[u8; BYTES_PER_BLOB], FftError> {
    if blob.len() != FIELD_ELEMENTS_PER_BLOB {
        return Err(FftError::InvalidBlobSize(blob.len()));
    }
    let flattened_blob = blob
        .iter()
        .flat_map(|x| pad_bytes(x.into_bigint().to_bytes_be(), BYTES_PER_FIELD_ELEMENT))
        .collect::<Vec<_>>();
    let flattened_blob_bytes = flattened_blob.len();
    flattened_blob.try_into().map_err(|_| FftError::InvalidBytesInBlob(flattened_blob_bytes))
}

pub fn deserialize_blob(blob_bytes: &[u8; BYTES_PER_BLOB]) -> [Fr; FIELD_ELEMENTS_PER_BLOB] {
    blob_bytes
        .chunks_exact(BYTES_PER_FIELD_ELEMENT)
        .map(|slice| Fr::from(BigUint::from_bytes_be(slice)))
        .collect::<Vec<Fr>>()
        .try_into()
        .expect("BYTES_PER_BLOB/BYTES_PER_FIELD_ELEMENT is FIELD_ELEMENTS_PER_BLOB")
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

pub(crate) fn polynomial_coefficients_to_blob(
    coefficients: Vec<Fr>,
) -> Result<[u8; BYTES_PER_BLOB], FftError> {
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

/// Structure to hold blob artifacts: commitments, proofs, and versioned hashes.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LegacyBlobArtifacts {
    #[serde_as(as = "Vec<Bytes>")]
    pub commitments: Vec<[u8; 48]>,
    #[serde_as(as = "Vec<Bytes>")]
    pub proofs: Vec<[u8; 48]>,
    #[serde_as(as = "Vec<Bytes>")]
    pub versioned_hashes: Vec<[u8; 32]>,
}

/// Computes a versioned hash from a KZG commitment.
fn kzg_to_versioned_hash(commitment: &KzgCommitment) -> [u8; 32] {
    const BLOB_COMMITMENT_VERSION_KZG: u8 = 0x01;

    // Get commitment bytes (48 bytes).
    let commitment_bytes = commitment.to_bytes();

    // Compute SHA256 of the commitment.
    let mut hasher = Sha256::new();
    hasher.update(commitment_bytes.as_ref());
    let mut hash = hasher.finalize();
    hash[0] = BLOB_COMMITMENT_VERSION_KZG;

    hash.into()
}

/// Computes KZG commitments, legacy proofs, and versioned hashes for a list of raw blobs.
///
/// For each blob, computes the KZG commitment and the corresponding KZG proof that is used
/// to verify the commitment. Returns `LegacyBlobArtifacts` structure.
pub fn compute_legacy_blob_commitments(
    raw_blobs: Vec<Vec<u8>>,
) -> Result<LegacyBlobArtifacts, FftError> {
    let mut commitments = Vec::new();
    let mut proofs = Vec::new();
    let mut versioned_hashes = Vec::new();

    for raw_blob in raw_blobs.iter() {
        // Convert raw blob bytes to Blob.
        let blob = Blob::from_bytes(raw_blob)?;

        // Compute KZG commitment.
        let commitment = blob_to_kzg_commitment(&blob)?;

        // Compute KZG proof.
        let proof = KZG_SETTINGS.compute_blob_kzg_proof(&blob, &commitment.to_bytes())?;

        // Compute versioned hash.
        let versioned_hash = kzg_to_versioned_hash(&commitment);

        commitments.push(*commitment);
        proofs.push(*proof);
        versioned_hashes.push(versioned_hash);
    }

    Ok(LegacyBlobArtifacts { commitments, proofs, versioned_hashes })
}

/// Structure to hold blob artifacts: commitments, cell proofs (CELLS_PER_EXT_BLOB per blob), and
/// versioned hashes.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlobArtifacts {
    #[serde_as(as = "Vec<Bytes>")]
    pub commitments: Vec<[u8; 48]>,
    #[serde_as(as = "Vec<Bytes>")]
    pub cell_proofs: Vec<[u8; 48]>,
    #[serde_as(as = "Vec<Bytes>")]
    pub versioned_hashes: Vec<[u8; 32]>,
}

/// Computes KZG commitments, cell proofs, and versioned hashes for a list of raw blobs.
///
/// For each blob, computes the KZG commitment and the corresponding KZG cell proofs that is used
/// to verify the commitment. Returns the internal `CellBlobs` structure with native KZG types.
pub fn compute_blob_commitments(raw_blobs: Vec<Vec<u8>>) -> Result<BlobArtifacts, FftError> {
    let mut commitments = Vec::new();
    let mut cell_proofs = Vec::new();
    let mut versioned_hashes = Vec::new();

    for raw_blob in raw_blobs.iter() {
        // Convert raw blob bytes to Blob.
        let blob = Blob::from_bytes(raw_blob)?;

        // Compute KZG commitment.
        let commitment = blob_to_kzg_commitment(&blob)?;

        // Compute KZG cell proofs.
        let (_, blob_cell_proofs) = KZG_SETTINGS.compute_cells_and_kzg_proofs(&blob)?;

        // Compute versioned hash.
        let versioned_hash = kzg_to_versioned_hash(&commitment);

        commitments.push(*commitment);
        cell_proofs.extend(blob_cell_proofs.into_iter().map(|proof| *proof));
        versioned_hashes.push(versioned_hash);
    }

    Ok(BlobArtifacts { commitments, cell_proofs, versioned_hashes })
}

pub fn decode_blobs(raw_blobs: Vec<[u8; BYTES_PER_BLOB]>) -> Result<Vec<Felt>, FftError> {
    let mut result = Vec::new();

    for raw_blob in raw_blobs.iter() {
        let mut coeffs: Vec<Fr> = deserialize_blob(raw_blob).into();

        bit_reversal(&mut coeffs)?;
        let domain = Radix2EvaluationDomain::<Fr>::new(FIELD_ELEMENTS_PER_BLOB)
            .ok_or(FftError::EvalDomainCreation)?;
        domain.ifft_in_place(&mut coeffs);

        for fr_elem in coeffs {
            let bytes = fr_elem.into_bigint().to_bytes_be();
            result.push(Felt::from_bytes_be_slice(&bytes));
        }
    }
    Ok(result)
}
