use ark_bls12_381::Fr;
use rand::Rng;
use starknet_types_core::curve::AffinePoint;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::kzg::utils::{
    polynomial_coefficients_to_blob,
    FIELD_ELEMENTS_PER_BLOB,
};
use crate::hints::hint_implementation::state_diff_encryption::utils::{
    compute_starknet_public_keys,
    decrypt_state_diff,
    decrypt_state_diff_from_blobs,
    encrypt_state_diff,
    encrypt_symmetric_key,
};
use crate::io::os_output_types::{PartialOsStateDiff, TryFromOutputIter};

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

#[test]
fn test_decrypt_state_diff_from_blobs() {
    let mut rng = rand::thread_rng();

    // Unencrypted DA segment of output with `full_output=false` and `use_kzg_da=true`.
    let da_segment = vec![
        Felt::from_hex("0x60001400000000010000000001000050002100000").unwrap(),
        Felt::from_hex("0x1275130f95dda36bcbb6e9d28796c1d7e10b6e9fd5ed083e0ede4b12f613528")
            .unwrap(),
        Felt::from_hex("0x3291859abec6454596859fa7f51688b15d4b94d181eab1b0f6c44f070cee406")
            .unwrap(),
        Felt::from_hex("0x723973208639b7839ce298f7ffea61e3f9533872defd7abdb91023db4658812")
            .unwrap(),
        Felt::from_hex("0x7368e36c028305eeec3e9c87373deafc49e7fedc20692e8dcfb16f7d409ddf5")
            .unwrap(),
        Felt::from_hex("0x2833c37e53489206582153747f19c4385079c6a72d8252483c1b6e043ab8b5d")
            .unwrap(),
        Felt::from_hex("0x1ed09bead87c025bb625d13b19800").unwrap(),
        Felt::from_hex("0x11d22c06ec4e6800").unwrap(),
        Felt::from_hex("0x112022c046808f011c0240000001200041f4c3e487d20f9000280008005").unwrap(),
        Felt::from_hex("0xc2001c801008c").unwrap(),
        Felt::from_hex("0x2d756ff").unwrap(),
        Felt::from_hex("0x38e90493bb0a8c19447d3f6").unwrap(),
    ];

    let original_state_diff =
        PartialOsStateDiff::try_from_output_iter(&mut da_segment.clone().into_iter())
            .expect("Failed to parse DA segment into PartialOsStateDiff");

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

    // Encrypt the DA segment
    let encrypted_state_diff = encrypt_state_diff(symmetric_key, &da_segment);
    let encrypted_symmetric_key =
        encrypt_symmetric_key(&sn_private_keys, &public_keys, symmetric_key);
    let sn_public_keys = compute_starknet_public_keys(&sn_private_keys);

    // Build full encrypted DA segment.
    let full_da_segment: Vec<Felt> = [Felt::from(n_keys)]
        .into_iter()
        .chain(sn_public_keys)
        .chain(encrypted_symmetric_key)
        .chain(encrypted_state_diff)
        .collect();

    // Convert to blobs.
    let da_segment_fr: Vec<Fr> =
        full_da_segment.into_iter().map(|felt| Fr::from(felt.to_biguint())).collect();

    let blobs: Vec<Vec<u8>> = da_segment_fr
        .chunks(FIELD_ELEMENTS_PER_BLOB)
        .map(|chunk| polynomial_coefficients_to_blob(chunk.to_vec()).unwrap())
        .collect();

    let decrypted_state_diff = decrypt_state_diff_from_blobs(blobs, private_keys[0], 0)
        .expect("Failed to decrypt and parse state diff from blobs");

    assert_eq!(
        decrypted_state_diff, original_state_diff,
        "Decrypted state diff should match original"
    );
}
