//! Generates a random private/public key pair for state diff encryption.
//!
//! Usage:
//!   cargo run --package starknet_os --bin generate_key_pairs
//!
//! The script generates a random private key and computes the corresponding
//! public key using the `compute_public_keys` function. Both hex and decimal
//! representations are printed for convenience.

use rand::Rng;
use starknet_os::hints::hint_implementation::state_diff_encryption::utils::compute_public_keys;
use starknet_types_core::felt::Felt;

fn main() {
    let mut rng = rand::thread_rng();

    // Generate a random private key
    let private_key = Felt::from_bytes_be(&rng.gen::<[u8; 32]>());

    // Compute public key using compute_public_keys
    let public_keys = compute_public_keys(&[private_key]);
    let public_key = public_keys[0];

    // Output the key pair
    println!("Generated key pair:\n");
    println!("Private Key:");
    println!("  Hex:    0x{:064x}", private_key);
    println!("  Decimal: {}", private_key);
    println!();
    println!("Public Key:");
    println!("  Hex:    0x{:064x}", public_key);
    println!("  Decimal: {}", public_key);
}
