use apollo_network_types::network_types::PeerId;
use hex::FromHex;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::block::BlockHash;
use starknet_api::core::Nonce;
use starknet_api::{felt, nonce};
use starknet_core::crypto::Signature;
use starknet_core::types::Felt;

use crate::signature_manager::{
    verify_identity,
    verify_precommit_vote_signature,
    LocalKeyStore,
    SignatureManager,
};

const ALICE_IDENTITY_SIGNATURE: Signature = Signature {
    r: Felt::from_hex_unchecked(
        "0x4c275c7fae888bf8ce0f43e938ee63b48a719a82c44f5991bf8be407ebfb390",
    ),
    s: Felt::from_hex_unchecked(
        "0x41640caf231352af1efa2f7d608f99a3a93e58ece2edcd2cdf200c86bf17232",
    ),
};

const ALICE_PRECOMMIT_SIGNATURE: Signature = Signature {
    r: Felt::from_hex_unchecked("0xe16ecc38c135735e8aed7ffdb150ebb956a93ec19ac53e8295cdbd04d552b2"),
    s: Felt::from_hex_unchecked(
        "0x4de081a9459b0e7defc49f7166f8869b33313020a20ffcc97506b8df6c42a7b",
    ),
};

#[derive(Clone, Debug)]
struct PeerIdentity {
    pub peer_id: PeerId,
    pub nonce: Nonce,
}

impl PeerIdentity {
    pub fn new() -> Self {
        // TODO(Elin): use a test util once it's merged.
        let peer_id =
            Vec::from_hex("00205cccc292b9dcc77610797e5f47b23d2b0fb7b77010d76481fc2c0652f6ca2fc2")
                .unwrap();

        Self { peer_id: PeerId::from_bytes(&peer_id).unwrap(), nonce: nonce!(0x1234) }
    }
}

#[rstest]
#[case::valid_signature(ALICE_IDENTITY_SIGNATURE, true)]
#[case::invalid_signature(
    Signature { r: felt!("0x1"), s: felt!("0x2") },
    false
)]
fn test_verify_identity(#[case] signature: Signature, #[case] expected: bool) {
    let PeerIdentity { peer_id, nonce } = PeerIdentity::new();
    let public_key = LocalKeyStore::new_for_testing().public_key;

    assert_eq!(verify_identity(peer_id, nonce, signature.into(), public_key).unwrap(), expected);
}

#[rstest]
#[case::valid_signature(ALICE_PRECOMMIT_SIGNATURE, true)]
#[case::invalid_signature(
    Signature { r: felt!("0x1"), s: felt!("0x2") },
    false
)]
fn test_verify_precommit_vote_signature(#[case] signature: Signature, #[case] expected: bool) {
    use starknet_api::block::BlockHash;

    let block_hash = BlockHash(felt!("0x1234"));
    let public_key = LocalKeyStore::new_for_testing().public_key;

    assert_eq!(
        verify_precommit_vote_signature(block_hash, signature.into(), public_key).unwrap(),
        expected
    );
}

#[tokio::test]
async fn test_identify() {
    let key_store = LocalKeyStore::new_for_testing();
    let signature_manager = SignatureManager::new(key_store);

    let PeerIdentity { peer_id, nonce } = PeerIdentity::new();
    let signature = signature_manager.identify(peer_id, nonce).await;

    assert_eq!(signature, Ok(ALICE_IDENTITY_SIGNATURE.into()));

    // Test alignment with verification function.
    assert_eq!(
        verify_identity(peer_id, nonce, signature.unwrap(), key_store.public_key).unwrap(),
        true
    );
}

#[tokio::test]
async fn test_sign_precommit_vote() {
    let key_store = LocalKeyStore::new_for_testing();
    let signature_manager = SignatureManager::new(key_store);

    let block_hash = BlockHash(felt!("0x1234"));
    let signature = signature_manager.sign_precommit_vote(block_hash).await;

    assert_eq!(signature, Ok(ALICE_PRECOMMIT_SIGNATURE.into()));

    // Test alignment with verification function.
    assert_eq!(
        verify_precommit_vote_signature(block_hash, signature.unwrap(), key_store.public_key)
            .unwrap(),
        true
    );
}
