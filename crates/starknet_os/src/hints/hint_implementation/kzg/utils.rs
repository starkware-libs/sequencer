use std::num::ParseIntError;
use std::sync::LazyLock;

use ark_bls12_381::Fr;
use ark_ff::{BigInteger, PrimeField};
use ark_poly::{EvaluationDomain, Radix2EvaluationDomain};
use c_kzg::{Blob, KzgCommitment, KzgProof, KzgSettings, BYTES_PER_FIELD_ELEMENT};
use num_bigint::{BigInt, BigUint, ParseBigIntError};
use num_traits::{Num, Signed, Zero};
use serde::{Deserialize, Serialize};
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

/// Structure to hold blob data, commitments, proofs, and versioned hashes.
pub struct Blobs {
    pub blobs: Vec<Vec<u8>>,
    pub commitments: Vec<KzgCommitment>,
    pub proofs: Vec<KzgProof>,
    pub versioned_hashes: Vec<[u8; 32]>,
}

/// Serializable structure to hold blob data, commitments, proofs, and versioned hashes.
/// All cryptographic objects are converted to byte arrays for easy serialization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SerializableBlobs {
    pub blobs: Vec<Vec<u8>>,
    pub commitments: Vec<Vec<u8>>,      // 48 bytes each
    pub proofs: Vec<Vec<u8>>,           // 48 bytes each
    pub versioned_hashes: Vec<Vec<u8>>, // 32 bytes each
}

impl From<Blobs> for SerializableBlobs {
    fn from(blobs: Blobs) -> Self {
        Self {
            blobs: blobs.blobs,
            commitments: blobs
                .commitments
                .into_iter()
                .map(|commitment| commitment.to_bytes().as_ref().to_vec())
                .collect(),
            proofs: blobs
                .proofs
                .into_iter()
                .map(|proof| proof.to_bytes().as_ref().to_vec())
                .collect(),
            versioned_hashes: blobs
                .versioned_hashes
                .into_iter()
                .map(|hash| hash.to_vec())
                .collect(),
        }
    }
}

/// Computes a versioned hash from a KZG commitment.
fn kzg_to_versioned_hash(commitment: &KzgCommitment) -> [u8; 32] {
    const BLOB_COMMITMENT_VERSION_KZG: u8 = 0x01;

    // Get commitment bytes (48 bytes).
    let commitment_bytes = commitment.to_bytes();

    // Compute SHA256 of the commitment.
    let mut hasher = Sha256::new();
    hasher.update(commitment_bytes.as_ref());
    let hash = hasher.finalize();

    // Create versioned hash: version_byte + sha256[1:].
    let mut versioned_hash = [0u8; 32];
    versioned_hash[0] = BLOB_COMMITMENT_VERSION_KZG;
    versioned_hash[1..].copy_from_slice(&hash[1..]);

    versioned_hash
}

/// Computes KZG commitments, proofs, and versioned hashes for a list of raw blobs.
///
/// For each blob, computes the KZG commitment and the corresponding KZG proof that is used
/// to verify the commitment. Returns the internal `Blobs` structure with native KZG types.
pub fn compute_blob_commitments(raw_blobs: Vec<Vec<u8>>) -> Result<Blobs, FftError> {
    let mut blobs = Vec::new();
    let mut commitments = Vec::new();
    let mut proofs = Vec::new();
    let mut versioned_hashes = Vec::new();

    for raw_blob in raw_blobs.iter() {
        // Convert raw blob bytes to Blob.
        let blob = Blob::from_bytes(raw_blob)?;

        // Compute KZG commitment.
        let commitment = blob_to_kzg_commitment(&blob)?;

        // Compute KZG proof.
        let proof = KzgProof::compute_blob_kzg_proof(&blob, &commitment.to_bytes(), &KZG_SETTINGS)?;

        // Compute versioned hash.
        let versioned_hash = kzg_to_versioned_hash(&commitment);

        blobs.push(raw_blob.clone());
        commitments.push(commitment);
        proofs.push(proof);
        versioned_hashes.push(versioned_hash);
    }

    Ok(Blobs { blobs, commitments, proofs, versioned_hashes })
}
