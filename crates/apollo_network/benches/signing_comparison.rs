//! Benchmark comparing PeerID (libp2p Ed25519) signing vs StakerID (Starknet ECDSA) signing.
//!
//! This benchmark compares two signing approaches used in the Apollo sequencer:
//! 1. PeerID signing: Ed25519 via libp2p Keypair (used in Propeller protocol)
//! 2. StakerID signing: ECDSA on Starknet curve (used by SignatureManager for consensus)
//!
//! Run with: cargo bench --bench signing_comparison -p apollo_network

use apollo_propeller::signature::{sign_message_id, verify_message_id_signature, SIGNING_POSTFIX, SIGNING_PREFIX};
use apollo_propeller::types::MessageRoot;
use divan::{black_box, Bencher};
use libp2p::identity::Keypair;
use starknet_api::crypto::utils::{PrivateKey, PublicKey, RawSignature};
use starknet_core::crypto::{ecdsa_sign, ecdsa_verify};
use starknet_core::types::Felt;
use starknet_crypto::get_public_key;

fn main() {
    divan::main();
}

/// Benchmark group for raw signing operations (just the signing step).
#[divan::bench_group]
mod raw_signing {
    use super::*;

    /// Benchmark raw Ed25519 signing with libp2p Keypair (PeerID approach).
    #[divan::bench]
    fn peer_id_raw_sign(bencher: Bencher<'_, '_>) {
        // Generate a keypair once
        let keypair = Keypair::generate_ed25519();
        // Create a test message (32 bytes, typical hash size)
        let message = [0u8; 32];

        bencher.bench_local(|| {
            // Sign the message directly
            let _signature = black_box(keypair.sign(black_box(&message)).unwrap());
        });
    }

    /// Benchmark raw ECDSA signing on Starknet curve (StakerID approach).
    #[divan::bench]
    fn staker_id_raw_sign(bencher: Bencher<'_, '_>) {
        // Create a test private key
        let private_key = PrivateKey(Felt::from_hex_unchecked(
            "0x608bf2cdb1ad4138e72d2f82b8c5db9fa182d1883868ae582ed373429b7a133",
        ));
        // Create a test message digest (Felt)
        let message_digest =
            Felt::from_hex_unchecked("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef");

        bencher.bench_local(|| {
            // Sign the message digest with ECDSA
            let _signature = black_box(ecdsa_sign(black_box(&private_key), black_box(&message_digest)).unwrap());
        });
    }
}

/// Benchmark group for full signing flows (including message preparation).
#[divan::bench_group]
mod full_signing_flow {
    use super::*;

    /// Benchmark full PeerID signing flow: message construction + Ed25519 signing.
    ///
    /// This simulates the `sign_message_id` function used in Propeller:
    /// - Concatenates SIGNING_PREFIX + message_id + SIGNING_POSTFIX
    /// - Signs with libp2p Keypair
    #[divan::bench]
    fn peer_id_full_flow(bencher: Bencher<'_, '_>) {
        // Generate a keypair once
        let keypair = Keypair::generate_ed25519();
        // Create a test message root (32-byte hash)
        let message_root = MessageRoot([0u8; 32]);

        bencher.bench_local(|| {
            // Use the actual sign_message_id function
            let _signature = black_box(sign_message_id(black_box(&message_root), black_box(&keypair)).unwrap());
        });
    }

    /// Benchmark full StakerID signing flow: blake2s hashing + ECDSA signing.
    ///
    /// This simulates the SignatureManager's signing flow:
    /// - Constructs message with domain separator
    /// - Hashes with blake2s to create Felt digest
    /// - Signs with ECDSA
    #[divan::bench]
    fn staker_id_full_flow(bencher: Bencher<'_, '_>) {
        // Create a test private key (same as LocalKeyStore uses)
        let private_key = PrivateKey(Felt::from_hex_unchecked(
            "0x608bf2cdb1ad4138e72d2f82b8c5db9fa182d1883868ae582ed373429b7a133",
        ));
        // Create test data (simulating peer identity message)
        let domain_separator = b"INIT_PEER_ID";
        let peer_id_bytes = [1u8; 38]; // Typical PeerId size
        let nonce_bytes = [0u8; 32];

        bencher.bench_local(|| {
            // Build the message (domain separator + peer_id + nonce)
            let mut message =
                Vec::with_capacity(domain_separator.len() + peer_id_bytes.len() + nonce_bytes.len());
            message.extend_from_slice(domain_separator);
            message.extend_from_slice(&peer_id_bytes);
            message.extend_from_slice(&nonce_bytes);

            // Hash with blake2s to create Felt digest
            let message_digest = blake2s_to_felt(&message);

            // Sign with ECDSA
            let _signature = black_box(ecdsa_sign(black_box(&private_key), black_box(&message_digest)).unwrap());
        });
    }

    /// Benchmark PeerID signing with manual message construction (for comparison).
    ///
    /// This manually constructs the message like sign_message_id does,
    /// to isolate the signing performance from the function call overhead.
    #[divan::bench]
    fn peer_id_manual_flow(bencher: Bencher<'_, '_>) {
        let keypair = Keypair::generate_ed25519();
        let message_id = [0u8; 32];

        bencher.bench_local(|| {
            // Manually construct the message
            let msg = [SIGNING_PREFIX, &message_id, SIGNING_POSTFIX].concat();
            // Sign
            let _signature = black_box(keypair.sign(black_box(&msg)).unwrap());
        });
    }
}

/// Benchmark group for raw verification operations (just the verification step).
#[divan::bench_group]
mod raw_verification {
    use super::*;

    /// Benchmark raw Ed25519 signature verification with libp2p PublicKey (PeerID approach).
    #[divan::bench]
    fn peer_id_raw_verify(bencher: Bencher<'_, '_>) {
        // Generate a keypair and sign a message once
        let keypair = Keypair::generate_ed25519();
        let public_key = keypair.public();
        let message = [0u8; 32];
        let signature = keypair.sign(&message).unwrap();

        bencher.bench_local(|| {
            // Verify the signature
            let _valid = black_box(public_key.verify(black_box(&message), black_box(&signature)));
        });
    }

    /// Benchmark raw ECDSA signature verification on Starknet curve (StakerID approach).
    #[divan::bench]
    fn staker_id_raw_verify(bencher: Bencher<'_, '_>) {
        // Create a test private key and sign a message once
        let private_key = PrivateKey(Felt::from_hex_unchecked(
            "0x608bf2cdb1ad4138e72d2f82b8c5db9fa182d1883868ae582ed373429b7a133",
        ));
        let public_key = PublicKey(get_public_key(&private_key));
        let message_digest =
            Felt::from_hex_unchecked("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef");
        let extended_signature = ecdsa_sign(&private_key, &message_digest).unwrap();
        // Convert ExtendedSignature to RawSignature to Signature
        let raw_signature = RawSignature::from(extended_signature);
        let signature: starknet_crypto::Signature = raw_signature.try_into().unwrap();

        bencher.bench_local(|| {
            // Verify the signature with ECDSA
            let _valid = black_box(
                ecdsa_verify(black_box(&public_key), black_box(&message_digest), black_box(&signature)).unwrap(),
            );
        });
    }
}

/// Benchmark group for full verification flows (including message preparation).
#[divan::bench_group]
mod full_verification_flow {
    use super::*;

    /// Benchmark full PeerID verification flow: message construction + Ed25519 verification.
    ///
    /// This simulates the `verify_message_id_signature` function used in Propeller:
    /// - Concatenates SIGNING_PREFIX + message_id + SIGNING_POSTFIX
    /// - Verifies with libp2p PublicKey
    #[divan::bench]
    fn peer_id_full_flow(bencher: Bencher<'_, '_>) {
        // Generate a keypair and sign a message once
        let keypair = Keypair::generate_ed25519();
        let public_key = keypair.public();
        let message_root = MessageRoot([0u8; 32]);
        let signature = sign_message_id(&message_root, &keypair).unwrap();

        bencher.bench_local(|| {
            // Use the actual verify_message_id_signature function
            let _result = black_box(
                verify_message_id_signature(black_box(&message_root), black_box(&signature), black_box(&public_key))
                    .is_ok(),
            );
        });
    }

    /// Benchmark full StakerID verification flow: blake2s hashing + ECDSA verification.
    ///
    /// This simulates the SignatureManager's verification flow:
    /// - Constructs message with domain separator
    /// - Hashes with blake2s to create Felt digest
    /// - Verifies with ECDSA
    #[divan::bench]
    fn staker_id_full_flow(bencher: Bencher<'_, '_>) {
        // Create a test private key and sign a message once
        let private_key = PrivateKey(Felt::from_hex_unchecked(
            "0x608bf2cdb1ad4138e72d2f82b8c5db9fa182d1883868ae582ed373429b7a133",
        ));
        let public_key = PublicKey(get_public_key(&private_key));

        // Create test data (simulating peer identity message)
        let domain_separator = b"INIT_PEER_ID";
        let peer_id_bytes = [1u8; 38]; // Typical PeerId size
        let nonce_bytes = [0u8; 32];

        // Build and sign the message once
        let mut message =
            Vec::with_capacity(domain_separator.len() + peer_id_bytes.len() + nonce_bytes.len());
        message.extend_from_slice(domain_separator);
        message.extend_from_slice(&peer_id_bytes);
        message.extend_from_slice(&nonce_bytes);
        let message_digest = blake2s_to_felt(&message);
        let extended_signature = ecdsa_sign(&private_key, &message_digest).unwrap();
        // Convert ExtendedSignature to RawSignature to Signature
        let raw_signature = RawSignature::from(extended_signature);
        let signature: starknet_crypto::Signature = raw_signature.try_into().unwrap();

        bencher.bench_local(|| {
            // Rebuild the message digest (as verification would do)
            let mut message =
                Vec::with_capacity(domain_separator.len() + peer_id_bytes.len() + nonce_bytes.len());
            message.extend_from_slice(domain_separator);
            message.extend_from_slice(&peer_id_bytes);
            message.extend_from_slice(&nonce_bytes);
            let message_digest = blake2s_to_felt(&message);

            // Verify with ECDSA
            let _valid = black_box(
                ecdsa_verify(black_box(&public_key), black_box(&message_digest), black_box(&signature)).unwrap(),
            );
        });
    }

    /// Benchmark PeerID verification with manual message construction (for comparison).
    ///
    /// This manually constructs the message like verify_message_id_signature does,
    /// to isolate the verification performance from the function call overhead.
    #[divan::bench]
    fn peer_id_manual_flow(bencher: Bencher<'_, '_>) {
        let keypair = Keypair::generate_ed25519();
        let public_key = keypair.public();
        let message_id = [0u8; 32];

        // Sign once
        let msg = [SIGNING_PREFIX, &message_id, SIGNING_POSTFIX].concat();
        let signature = keypair.sign(&msg).unwrap();

        bencher.bench_local(|| {
            // Manually construct the message
            let msg = [SIGNING_PREFIX, &message_id, SIGNING_POSTFIX].concat();
            // Verify
            let _valid = black_box(public_key.verify(black_box(&msg), black_box(&signature)));
        });
    }
}

// Helper function to convert blake2s hash to Felt (copied from apollo_signature_manager)
fn blake2s_to_felt(data: &[u8]) -> Felt {
    use blake2::digest::{Update, VariableOutput};
    use blake2::Blake2sVar;

    let mut hasher = Blake2sVar::new(32).unwrap();
    hasher.update(data);
    let mut result = [0u8; 32];
    hasher.finalize_variable(&mut result).unwrap();
    Felt::from_bytes_be(&result)
}
