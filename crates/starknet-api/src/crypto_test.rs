// Unittest for verify_message_signature

use crate::crypto::{verify_message_hash_signature, PublicKey, Signature};
use crate::hash::{poseidon_hash_array, StarkFelt};
use crate::stark_felt;

#[test]
fn signature_verification() {
    // The signed message of block 4256.
    let message_hash = poseidon_hash_array(&[
        stark_felt!("0x7d5db04c5ca2aea828180dc441afb1580e3cee7547a3567ced3aa5bb8b273c0"),
        stark_felt!("0x64689c12248e1110af4b3af0e2b43cd51ad13e8855f10e37669e2a4baf919c6"),
    ]);
    // The signature of the message.
    let signature = Signature {
        r: stark_felt!("0x1b382bbfd693011c9b7692bc932b23ed9c288deb27c8e75772e172abbe5950c"),
        s: stark_felt!("0xbe4438085057e1a7c704a0da3b30f7b8340fe3d24c86772abfd24aa597e42"),
    };
    // The public key of the sequencer.
    let public_key =
        PublicKey(stark_felt!("0x48253ff2c3bed7af18bde0b611b083b39445959102d4947c51c4db6aa4f4e58"));

    let result = verify_message_hash_signature(&message_hash.0, &signature, &public_key).unwrap();
    assert!(result);
}
