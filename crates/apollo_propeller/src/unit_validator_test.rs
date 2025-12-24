use std::sync::Arc;

use libp2p::identity::Keypair;
use libp2p::PeerId;
use rstest::{fixture, rstest};

use crate::types::ShardSignatureVerificationError;
use crate::{
    Channel,
    MessageRoot,
    PropellerScheduleManager,
    PropellerUnit,
    ShardIndex,
    ShardValidationError,
    UnitValidator,
};

struct TestEnv {
    channel: Channel,
    message_root: MessageRoot,
    keypair: Keypair,
    publisher: PeerId,
    #[allow(unused)] // TODO(AndrewL): remove this once we use it
    local_peer: PeerId,
    tree_manager: Arc<PropellerScheduleManager>,
}

#[fixture]
fn env() -> TestEnv {
    let channel = Channel(1);
    let message_root = MessageRoot([1u8; 32]);
    let keypair = Keypair::generate_ed25519();
    let publisher = PeerId::from(keypair.public());
    let local_peer = PeerId::random();
    let peers = vec![
        (local_peer, 100),
        (publisher, 80),
        (PeerId::random(), 60),
        (PeerId::random(), 40),
        (PeerId::random(), 20),
    ];
    let tree_manager = Arc::new(PropellerScheduleManager::new(local_peer, peers.clone()).unwrap());
    TestEnv { channel, message_root, keypair, publisher, local_peer, tree_manager }
}

fn custom_unit(env: &TestEnv, index: ShardIndex, tampered_signature: bool) -> PropellerUnit {
    let mut correct_signature =
        crate::signature::sign_message_id(&env.message_root, &env.keypair).unwrap();
    let signature = if tampered_signature {
        *correct_signature.last_mut().unwrap() += 1;
        correct_signature
    } else {
        correct_signature
    };
    PropellerUnit::new(
        env.channel,
        env.publisher,
        env.message_root,
        signature,
        index,
        vec![1, 2, 3],
        crate::MerkleProof { siblings: vec![] },
    )
}

fn unit(env: &TestEnv, index: ShardIndex) -> PropellerUnit {
    custom_unit(env, index, false)
}

#[rstest]
fn test_validation_of_legal_unit(env: TestEnv) {
    let unit = unit(&env, ShardIndex(0));
    let mut validator = UnitValidator::new(
        env.channel,
        env.publisher,
        env.keypair.public(),
        env.message_root,
        env.tree_manager,
    );
    validator.validate_shard(env.publisher, &unit).ok();
}

#[rstest]
fn test_validation_fails_with_wrong_signature(env: TestEnv) {
    let unit = custom_unit(&env, ShardIndex(0), true);
    let mut validator = UnitValidator::new(
        env.channel,
        env.publisher,
        env.keypair.public(),
        env.message_root,
        env.tree_manager.clone(),
    );
    assert!(matches!(
        validator.validate_shard(env.publisher, &unit),
        Err(ShardValidationError::SignatureVerificationFailed(
            ShardSignatureVerificationError::VerificationFailed
        ))
    ));
}

#[rstest]
fn test_duplicate_shard_rejected(env: TestEnv) {
    let unit = unit(&env, ShardIndex(0));
    let mut validator = UnitValidator::new(
        env.channel,
        env.publisher,
        env.keypair.public(),
        env.message_root,
        env.tree_manager.clone(),
    );

    validator.validate_shard(env.publisher, &unit).ok();
    assert!(matches!(
        validator.validate_shard(env.publisher, &unit),
        Err(ShardValidationError::DuplicateShard)
    ));
}
