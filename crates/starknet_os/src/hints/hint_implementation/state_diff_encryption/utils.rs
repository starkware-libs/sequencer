use blake2s::encode_felt252_data_and_calc_blake_hash;
use lambdaworks_math::elliptic_curve::short_weierstrass::curves::stark_curve::StarkCurve;
use lambdaworks_math::elliptic_curve::short_weierstrass::traits::IsShortWeierstrass;
use starknet_types_core::curve::AffinePoint;
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "utils_test.rs"]
mod utils_test;

/// Recovers the corresponding y coordinate on the elliptic curve
/// y^2 = x^3 + alpha * x + beta (mod field_prime)
/// of a given x coordinate, where alpha and beta are the Starknet curve parameters.
// TODO(Avi, 10/09/2025): Remove this and use [AffinePoint::new_from_x] instead after bumping
// starknet-types-core to 0.2.0.
#[allow(dead_code)]
fn recover_y(x: Felt) -> Felt {
    let alpha = Felt::from_bytes_be(&StarkCurve::a().to_bytes_be());
    let beta = Felt::from_bytes_be(&StarkCurve::b().to_bytes_be());
    let y_squared = x.pow(3_u128) + alpha * x + beta;
    y_squared.sqrt().expect("{x} does not represent the x coordinate of a point on the curve.")
}

/// Computes elliptic curve public keys from private keys using the generator point.
/// Returns only the x-coordinates of the resulting public key points.
#[allow(dead_code)]
pub fn compute_public_keys(private_keys: &[Felt]) -> Vec<Felt> {
    let mut public_keys = Vec::with_capacity(private_keys.len());
    for &private_key in private_keys {
        let public_key_point = &AffinePoint::generator() * private_key;
        public_keys.push(public_key_point.x());
    }
    public_keys
}

/// Encrypts a symmetric key for multiple recipients using Diffie-Hellman key exchange.
/// Returns one encrypted version of the symmetric key for each recipient.
#[allow(dead_code)]
pub fn encrypt_symmetric_key(
    sn_private_keys: &[Felt],
    public_keys: &[Felt],
    symmetric_key: Felt,
) -> Vec<Felt> {
    assert_eq!(sn_private_keys.len(), public_keys.len());

    sn_private_keys
        .iter()
        .zip(public_keys)
        .map(|(&sn_private_key, &public_key)| {
            let y = recover_y(public_key);
            let public_key_point = AffinePoint::new(public_key, y).expect("Invalid public key");
            let shared_secret = (&public_key_point * sn_private_key).x();
            // Encrypt the symmetric key using the shared secret.
            // TODO(Avi, 10/09/2025): Use the naive felt encoding once available.
            symmetric_key + encode_felt252_data_and_calc_blake_hash(&[shared_secret])
        })
        .collect()
}

#[allow(dead_code)]
pub fn decrypt_state_diff(
    private_key: Felt,
    sn_public_key: Felt,
    encrypted_symmetric_key: Felt,
    encrypted_state_diff: &[Felt],
) -> Vec<Felt> {
    // Compute the shared secret using Diffie-Hellman key exchange.
    let sn_public_key_y = recover_y(sn_public_key);
    let sn_public_key_point =
        AffinePoint::new(sn_public_key, sn_public_key_y).expect("Invalid public key coordinates.");
    let shared_secret_point = &sn_public_key_point * private_key;
    let shared_secret = shared_secret_point.x();

    // Decrypt the symmetric key using the shared secret.
    // TODO(Avi, 10/09/2025): Use the naive felt encoding once avialable.
    let symmetric_key =
        encrypted_symmetric_key - encode_felt252_data_and_calc_blake_hash(&[shared_secret]);

    // Decrypt the state diff using the symmetric key.
    // TODO(Avi, 10/09/2025): Use the naive felt encoding once avialable.
    encrypted_state_diff
        .iter()
        .enumerate()
        .map(|(i, encrypted_felt)| {
            encrypted_felt
                - encode_felt252_data_and_calc_blake_hash(&[symmetric_key, Felt::from(i)])
        })
        .collect()
}
