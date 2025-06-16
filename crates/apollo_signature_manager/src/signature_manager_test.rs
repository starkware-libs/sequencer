use apollo_signature_manager_types::PeerId;
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
        "0x7687c83bdfa7474518c585f1b58a028b939764f1d2721e63bf821c4c8987299",
    ),
    s: Felt::from_hex_unchecked(
        "0x7e05746545ed1fe24fec988341d2452a4bbcebec26d73f9ee9bdc9426a372a5",
    ),
};

const ALICE_PRECOMMIT_SIGNATURE: Signature = Signature {
    r: Felt::from_hex_unchecked("0xcd59947811bac7c33d3dae3d50b1de243710b05f285455ada6823e23871a2b"),
    s: Felt::from_hex_unchecked(
        "0x33817fd47c5253c4979999afe0dd6b275498d9c7b96dd7705b84c2113228f11",
    ),
};

#[derive(Clone, Debug)]
struct PeerIdentity {
    pub peer_id: PeerId,
    pub nonce: Nonce,
}

impl PeerIdentity {
    pub fn new() -> Self {
        Self { peer_id: PeerId(b"alice".to_vec()), nonce: nonce!(0x1234) }
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

    assert_eq!(verify_identity(peer_id, nonce, signature.into(), public_key), Ok(expected));
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
        verify_precommit_vote_signature(block_hash, signature.into(), public_key),
        Ok(expected)
    );
}

#[tokio::test]
async fn test_identify() {
    let key_store = LocalKeyStore::new_for_testing();
    let signature_manager = SignatureManager::new(key_store);

    let PeerIdentity { peer_id, nonce } = PeerIdentity::new();
    let signature = signature_manager.identify(peer_id.clone(), nonce).await;

    assert_eq!(signature, Ok(ALICE_IDENTITY_SIGNATURE.into()));

    // Test alignment with verification function.
    assert_eq!(verify_identity(peer_id, nonce, signature.unwrap(), key_store.public_key), Ok(true));
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
        verify_precommit_vote_signature(block_hash, signature.unwrap(), key_store.public_key),
        Ok(true)
    );
}
