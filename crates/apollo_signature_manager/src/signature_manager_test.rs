use apollo_network_types::network_types::PeerId;
use apollo_signature_manager_types::{KeySourceConfig, SignatureManagerConfig};
use expect_test::expect_file;
use hex::FromHex;
use starknet_api::block::BlockHash;
use starknet_api::crypto::utils::{Challenge, PrivateKey, PublicKey};
use starknet_api::felt;
use starknet_core::crypto::Signature;
use starknet_core::types::Felt;
use starknet_crypto::get_public_key;

use crate::signature_manager::{
    verify_identity,
    verify_precommit_vote_signature,
    SignatureManager,
};

const TEST_PRIVATE_KEY: PrivateKey = PrivateKey(Felt::from_hex_unchecked(
    "0x608bf2cdb1ad4138e72d2f82b8c5db9fa182d1883868ae582ed373429b7a133",
));

fn test_config() -> SignatureManagerConfig {
    SignatureManagerConfig { key_source: KeySourceConfig::Local { private_key: TEST_PRIVATE_KEY } }
}

fn test_public_key() -> PublicKey {
    PublicKey(get_public_key(&TEST_PRIVATE_KEY))
}

#[derive(Clone, Debug)]
struct PeerIdentity {
    pub peer_id: PeerId,
    pub challenge: Challenge,
}

impl PeerIdentity {
    pub fn new() -> Self {
        // TODO(Elin): use a test util once it's merged.
        let peer_id =
            Vec::from_hex("00205cccc292b9dcc77610797e5f47b23d2b0fb7b77010d76481fc2c0652f6ca2fc2")
                .unwrap();

        Self { peer_id: PeerId::from_bytes(&peer_id).unwrap(), challenge: Challenge::from(0xdead) }
    }
}

#[test]
fn test_verify_identity_invalid_signature() {
    let PeerIdentity { peer_id, challenge } = PeerIdentity::new();
    let invalid_signature = Signature { r: felt!("0x1"), s: felt!("0x2") };

    assert!(
        !verify_identity(peer_id, challenge, invalid_signature.into(), test_public_key()).unwrap()
    );
}

#[test]
fn test_verify_precommit_vote_signature_invalid() {
    let block_hash = BlockHash(felt!("0x1234"));
    let invalid_signature = Signature { r: felt!("0x1"), s: felt!("0x2") };

    assert!(
        !verify_precommit_vote_signature(block_hash, invalid_signature.into(), test_public_key())
            .unwrap()
    );
}

#[tokio::test]
async fn test_sign_identification() {
    let signature_manager = SignatureManager::new(test_config()).unwrap();

    let PeerIdentity { peer_id, challenge } = PeerIdentity::new();
    let signature = signature_manager.sign_identification(peer_id, challenge).await.unwrap();

    // Auto-updates the file when you run: UPDATE_EXPECT=1 cargo test
    expect_file!["test_data/alice_identity_signature.txt"].assert_debug_eq(&signature);

    assert!(verify_identity(peer_id, challenge, signature, test_public_key()).unwrap());
}

#[tokio::test]
async fn test_sign_precommit_vote() {
    let signature_manager = SignatureManager::new(test_config()).unwrap();

    let block_hash = BlockHash(felt!("0x1234"));
    let signature = signature_manager.sign_precommit_vote(block_hash).await.unwrap();

    // Auto-updates the file when you run: UPDATE_EXPECT=1 cargo test
    expect_file!["test_data/alice_precommit_signature.txt"].assert_debug_eq(&signature);

    assert!(verify_precommit_vote_signature(block_hash, signature, test_public_key()).unwrap());
}
