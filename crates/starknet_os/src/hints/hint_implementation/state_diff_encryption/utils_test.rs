use num_traits::ToPrimitive;
use rand::Rng;
use starknet_types_core::curve::AffinePoint;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::state_diff_encryption::utils::{
    decrypt_state_diff,
    encrypt_state_diff,
    recover_y,
};

#[test]
fn test_recover_y() {
    let g = starknet_types_core::curve::AffinePoint::generator();
    for i in 1..16 {
        let p = &g * Felt::from(i);
        let x = p.x();
        let y = recover_y(x);
        assert_eq!(y * y, p.y() * p.y());
    }
}

#[test]
#[should_panic]
fn test_recover_y_not_on_curve() {
    let x = Felt::from(18);
    recover_y(x);
}

#[test]
fn test_encrypt_decrypt_roundtrip_random() {
    let mut rng = rand::thread_rng();

    // Random number of keys.
    let n_keys: usize = rng.gen_range(1..=5);

    // Generate private keys and corresponding public key x-coordinates.
    let mut private_keys: Vec<Felt> = Vec::with_capacity(n_keys);
    let mut public_keys: Vec<Felt> = Vec::with_capacity(n_keys);
    for _ in 0..n_keys {
        let private_key = Felt::from(rng.gen_range(1..=1_000_000));
        let public_key_x = (&AffinePoint::generator() * private_key).x();
        private_keys.push(private_key);
        public_keys.push(public_key_x);
    }

    // Generate SN private keys.
    let mut sn_private_keys: Vec<Felt> = Vec::with_capacity(n_keys);
    for _ in 0..n_keys {
        let sn_priv_scalar: u64 = rng.gen_range(1..=1_000_000);
        sn_private_keys.push(Felt::from(sn_priv_scalar));
    }

    // Random symmetric key.
    let symmetric_key = Felt::from_bytes_be(&rng.gen::<[u8; 32]>());

    // Random state diff.
    let state_diff_length: usize = rng.gen_range(0..=20);
    let mut state_diff: Vec<Felt> = Vec::with_capacity(state_diff_length);
    for _ in 0..state_diff_length {
        state_diff.push(Felt::from_bytes_be(&rng.gen::<[u8; 32]>()))
    }

    // Encrypt and then decrypt with every keypair.
    let encrypted = encrypt_state_diff(&public_keys, &sn_private_keys, symmetric_key, &state_diff);

    let n_keys = encrypted[0].to_usize().unwrap();
    let sn_public_keys = &encrypted[1..n_keys + 1];
    let symmetric_key_encryptions = &encrypted[n_keys + 1..(2 * n_keys) + 1];
    let encrypted_state_diff = &encrypted[(2 * n_keys) + 1..];

    for i in 0..n_keys {
        let decrypted = decrypt_state_diff(
            private_keys[i],
            sn_public_keys[i],
            symmetric_key_encryptions[i],
            encrypted_state_diff,
        );
        assert_eq!(decrypted, state_diff);
    }
}
