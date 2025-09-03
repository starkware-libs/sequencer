use blake2s::encode_felt252_data_and_calc_blake_hash;
use starknet_curve::curve_params::{ALPHA, BETA};
use starknet_types_core::curve::AffinePoint;
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "utils_test.rs"]
mod utils_test;

// TODO(Avi, 10/09/2025): Remove this and use [`AffinePoint::new_from_x`] instead after bumping
// starknet-types-core to 0.2.0.
fn recover_y(x: Felt) -> Felt {
    let alpha = Felt::from_bytes_be(&ALPHA.to_bytes_be());
    let beta = Felt::from_bytes_be(&BETA.to_bytes_be());
    let y_sq = x * x * x + alpha * x + beta;
    y_sq.sqrt().expect("x is not on the curve")
}

pub fn decrypt_state_diff(
    private_key: Felt,
    sn_public_key: Felt,
    encrypted_symmetric_key: Felt,
    encrypted_state_diff: &[Felt],
) -> Vec<Felt> {
    let sn_public_key_y = recover_y(sn_public_key);

    let sn_public_key_point = AffinePoint::new(sn_public_key, sn_public_key_y)
        .expect("invalid public key coordinates");
    let shared_key_point = &sn_public_key_point * private_key;
    let shared_key = shared_key_point.x();

    let symmetric_key = encrypted_symmetric_key
        - Felt::from(encode_felt252_data_and_calc_blake_hash(&[shared_key]));

    let mut decrypted_data = Vec::new();
    for (i, encrypted_felt) in encrypted_state_diff.iter().enumerate() {
        let decrypted_felt = encrypted_felt
            - Felt::from(encode_felt252_data_and_calc_blake_hash(&[
                symmetric_key,
                Felt::from(i),
            ]));
        decrypted_data.push(decrypted_felt);
    }

    decrypted_data
}


