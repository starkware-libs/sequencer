use std::collections::HashMap;
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
    signature: Vec<u8>,
    validator: UnitValidator,
    publisher: PeerId,
    local_peer: PeerId,
    other_peers: Vec<PeerId>,
    peer_to_index: HashMap<PeerId, ShardIndex>,
}

#[fixture]
fn env() -> TestEnv {
    let channel = Channel(1);
    let message_root = MessageRoot([1u8; 32]);
    let num_peers = 5;
    let keypair = Keypair::generate_ed25519();
    let publisher = PeerId::from(keypair.public());
    let local_peer = PeerId::random();
    let other_peers = (0..(num_peers - 2)).map(|_| PeerId::random()).collect::<Vec<_>>();

    let mut peers = vec![(local_peer, 10), (publisher, 10)];
    peers.extend(other_peers.iter().copied().map(|peer| (peer, 10)));

    let tree_manager = Arc::new(PropellerScheduleManager::new(local_peer, peers).unwrap());

    let mut peer_to_index = HashMap::new();
    for i in 0..(num_peers - 1) {
        let index = ShardIndex(i.try_into().unwrap());
        let peer = tree_manager.get_peer_for_shard_index(&publisher, index).unwrap();
        peer_to_index.insert(peer, index);
    }

    let validator =
        UnitValidator::new(channel, publisher, keypair.public(), message_root, tree_manager);
    let signature = crate::signature::sign_message_id(&message_root, &keypair).unwrap();

    TestEnv {
        channel,
        message_root,
        signature,
        validator,
        publisher,
        local_peer,
        other_peers,
        peer_to_index,
    }
}

fn custom_unit(env: &TestEnv, owner: PeerId, tampered_signature: bool) -> PropellerUnit {
    let index: ShardIndex = *env.peer_to_index.get(&owner).unwrap();
    let mut correct_signature = env.signature.clone();
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

fn unit(env: &TestEnv, owner: PeerId) -> PropellerUnit {
    custom_unit(env, owner, false)
}

#[rstest]
fn test_validation_of_legal_unit(mut env: TestEnv) {
    let unit = unit(&env, env.local_peer);
    env.validator.validate_shard(env.publisher, &unit).ok();
}

#[rstest]
fn test_validation_fails_with_wrong_signature(mut env: TestEnv) {
    let unit = custom_unit(&env, env.local_peer, true);
    assert!(matches!(
        env.validator.validate_shard(env.publisher, &unit),
        Err(ShardValidationError::SignatureVerificationFailed(
            ShardSignatureVerificationError::VerificationFailed
        ))
    ));
}

#[rstest]
fn test_duplicate_shard_rejected(mut env: TestEnv) {
    let unit = unit(&env, env.local_peer);
    env.validator.validate_shard(env.publisher, &unit).unwrap();
    assert!(matches!(
        env.validator.validate_shard(env.publisher, &unit),
        Err(ShardValidationError::DuplicateShard)
    ));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Sender {
    Publisher,
    LocalPeer,
    OtherPeer1,
    OtherPeer2,
    Random,
}

impl Sender {
    fn id(self, env: &TestEnv) -> PeerId {
        match self {
            Sender::Publisher => env.publisher,
            Sender::LocalPeer => env.local_peer,
            Sender::OtherPeer1 => env.other_peers[0],
            Sender::OtherPeer2 => env.other_peers[1],
            Sender::Random => PeerId::random(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UnitOwner {
    LocalPeer,
    OtherPeer1,
    OtherPeer2,
}

impl UnitOwner {
    fn id(self, env: &TestEnv) -> PeerId {
        match self {
            UnitOwner::LocalPeer => env.local_peer,
            UnitOwner::OtherPeer1 => env.other_peers[0],
            UnitOwner::OtherPeer2 => env.other_peers[1],
        }
    }
}

#[rstest]
fn test_unit_source_validation(
    mut env: TestEnv,
    #[values(
        Sender::Publisher,
        Sender::LocalPeer,
        Sender::OtherPeer1,
        Sender::OtherPeer2,
        Sender::Random
    )]
    sender: Sender,
    #[values(UnitOwner::LocalPeer, UnitOwner::OtherPeer1, UnitOwner::OtherPeer2)] owner: UnitOwner,
) {
    let my_unit = unit(&env, owner.id(&env));
    let sender_id = sender.id(&env);
    let result = env.validator.validate_shard(sender_id, &my_unit);
    let hop1 = (sender == Sender::Publisher) && (owner == UnitOwner::LocalPeer);
    let hop2_a = (sender == Sender::OtherPeer1) && (owner == UnitOwner::OtherPeer1);
    let hop2_b = (sender == Sender::OtherPeer2) && (owner == UnitOwner::OtherPeer2);
    if hop1 || hop2_a || hop2_b {
        assert_eq!(result, Ok(()));
    } else {
        result.unwrap_err();
    }
}
