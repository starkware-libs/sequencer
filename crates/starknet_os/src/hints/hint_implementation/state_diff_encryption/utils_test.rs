use rand::Rng;
use starknet_types_core::curve::AffinePoint;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::state_diff_encryption::utils::{
    compute_starknet_public_keys,
    decrypt_state_diff,
    encrypt_state_diff,
    encrypt_symmetric_key,
};

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
    let encrypted_state_diff = encrypt_state_diff(symmetric_key, &state_diff);
    let symmetric_key_encryptions =
        encrypt_symmetric_key(&sn_private_keys, &public_keys, symmetric_key);
    let sn_public_keys = compute_starknet_public_keys(&sn_private_keys);

    for i in 0..n_keys {
        let decrypted = decrypt_state_diff(
            private_keys[i],
            sn_public_keys[i],
            symmetric_key_encryptions[i],
            &encrypted_state_diff,
        );
        assert_eq!(decrypted, state_diff);
    }
}
