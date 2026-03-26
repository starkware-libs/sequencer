use std::collections::HashMap;
use std::sync::Arc;

use libp2p::identity::Keypair;
use libp2p::PeerId;
use rstest::rstest;
use starknet_api::staking::StakingWeight;

use crate::types::SignatureVerificationError;
use crate::{
    CommitteeId,
    MerkleTree,
    MessageRoot,
    PropellerScheduleManager,
    PropellerUnit,
    Shard,
    ShardIndex,
    ShardsOfPeer,
    UnitValidationError,
    UnitValidator,
};

struct TestEnv {
    committee_id: CommitteeId,
    message_root: MessageRoot,
    signature: Vec<u8>,
    validator: UnitValidator,
    publisher: PeerId,
    local_peer: PeerId,
    other_peers: Vec<PeerId>,
    merkle_tree: MerkleTree,
    peer_to_index: HashMap<PeerId, ShardIndex>,
    shards_of_peer: ShardsOfPeer,
}
const TEST_NONCE: u64 = 1_700_000_000_000_000_000;

fn make_shards(num_shards_per_peer: usize) -> ShardsOfPeer {
    let shards =
        (0..num_shards_per_peer).map(|i| Shard(vec![u8::try_from(i).unwrap(); 5])).collect();
    ShardsOfPeer(shards)
}

fn build_env(num_shards_per_peer: usize) -> TestEnv {
    const NUM_PEERS: usize = 5;
    const COMMITTEE: CommitteeId = CommitteeId([1u8; 32]);
    let keypair = Keypair::generate_ed25519();
    let publisher = PeerId::from(keypair.public());
    let local_peer = PeerId::random();
    let other_peers = (0..(NUM_PEERS - 2)).map(|_| PeerId::random()).collect::<Vec<_>>();

    let mut peers = vec![(local_peer, StakingWeight(10)), (publisher, StakingWeight(10))];
    peers.extend(other_peers.iter().copied().map(|peer| (peer, StakingWeight(10))));

    let schedule_manager = Arc::new(PropellerScheduleManager::new(local_peer, peers).unwrap());

    let mut peer_to_index = HashMap::new();
    for i in 0..(NUM_PEERS - 1) {
        // TODO(AndrewL): Instead of testing that you use schedule_manager properly by calling its
        // functions in the test, use automock and dependency injection
        let index = ShardIndex(i.try_into().unwrap());
        let peer = schedule_manager.get_peer_for_shard_index(&publisher, index).unwrap();
        peer_to_index.insert(peer, index);
    }

    let shards_of_peer = make_shards(num_shards_per_peer);

    // TODO(AndrewL): Use automock and dependency injection
    let leaf_data: Vec<Vec<u8>> =
        (0..(NUM_PEERS - 1)).map(|_| shards_of_peer.encode_to_proto_bytes()).collect();
    let merkle_tree = MerkleTree::new(&leaf_data);
    let message_root = MessageRoot(merkle_tree.root().unwrap());

    let validator =
        UnitValidator::new(COMMITTEE, publisher, keypair.public(), message_root, schedule_manager);
    let signature =
        crate::signature::sign_message_id(&message_root, COMMITTEE, TEST_NONCE, &keypair).unwrap();

    TestEnv {
        committee_id: COMMITTEE,
        message_root,
        signature,
        validator,
        publisher,
        local_peer,
        other_peers,
        peer_to_index,
        merkle_tree,
        shards_of_peer,
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
        env.committee_id,
        env.publisher,
        env.message_root,
        signature,
        index,
        env.shards_of_peer.clone(),
        env.merkle_tree.prove(index.0.try_into().unwrap()).unwrap(),
        TEST_NONCE,
    )
}

fn unit(env: &TestEnv, owner: PeerId) -> PropellerUnit {
    custom_unit(env, owner, false)
}

// TODO(AndrewL): Test positive flow of multiple shards per peer once it's supported.
#[rstest]
fn test_validation_of_legal_unit() {
    let mut env = build_env(1);
    let unit = unit(&env, env.local_peer);
    env.validator.validate_unit(env.publisher, &unit).unwrap();
}

#[rstest]
fn test_validation_fails_with_wrong_signature() {
    let mut env = build_env(1);
    let unit = custom_unit(&env, env.local_peer, true);
    assert!(matches!(
        env.validator.validate_unit(env.publisher, &unit),
        Err(UnitValidationError::SignatureVerificationFailed(
            SignatureVerificationError::VerificationFailed
        ))
    ));
}

#[rstest]
fn test_duplicate_shard_rejected() {
    let mut env = build_env(1);
    let unit = unit(&env, env.local_peer);
    env.validator.validate_unit(env.publisher, &unit).unwrap();
    assert!(matches!(
        env.validator.validate_unit(env.publisher, &unit),
        Err(UnitValidationError::DuplicateUnit)
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
    let mut env = build_env(1);
    let my_unit = unit(&env, owner.id(&env));
    let sender_id = sender.id(&env);
    let result = env.validator.validate_unit(sender_id, &my_unit);
    let hop1 = (sender == Sender::Publisher) && (owner == UnitOwner::LocalPeer);
    let hop2_a = (sender == Sender::OtherPeer1) && (owner == UnitOwner::OtherPeer1);
    let hop2_b = (sender == Sender::OtherPeer2) && (owner == UnitOwner::OtherPeer2);
    if hop1 || hop2_a || hop2_b {
        assert_eq!(result, Ok(()));
    } else {
        result.unwrap_err();
    }
}

#[rstest]
fn test_tampered_proof_fails_verification() {
    let mut env = build_env(1);
    let mut unit = unit(&env, env.local_peer);
    unit.shards_mut().0[0].0.push(42);

    let result = env.validator.validate_unit(env.publisher, &unit);
    result.unwrap_err();
}

#[rstest]
fn test_unequal_shard_lengths_rejected() {
    let env = build_env(2);
    let mut unit = unit(&env, env.local_peer);
    unit.shards_mut().0[1].0.push(0xFF);

    assert_eq!(unit.validate_shard_lengths(), Err(UnitValidationError::UnequalShardLengths));
}

#[rstest]
fn test_unexpected_shard_count_rejected() {
    let mut env = build_env(2);
    let unit = unit(&env, env.local_peer);

    assert_eq!(
        env.validator.validate_unit(env.publisher, &unit),
        Err(UnitValidationError::UnexpectedShardCount {
            expected_shard_count: 1,
            actual_shard_count: 2,
        })
    );
}

#[rstest]
fn test_merkle_proof_valid_with_multiple_shards() {
    let env = build_env(2);
    let unit = unit(&env, env.local_peer);

    unit.validate_shard_lengths().unwrap();
    unit.validate_merkle_proof(env.merkle_tree.leaf_count()).unwrap();
}
