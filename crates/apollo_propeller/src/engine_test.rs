use std::time::Duration;

use libp2p::identity::Keypair;
use libp2p::PeerId;
use starknet_api::staking::StakingWeight;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::engine::{Engine, EngineCommand, EngineOutput, MessageKey};
use crate::message_processor::{EventStateManagerToEngine, GoodShardsStatus};
use crate::types::{CommitteeId, MessageRoot};
use crate::{MerkleProof, PropellerUnit, Shard, ShardIndex, ShardsOfPeer};

const TEST_COMMITTEE_ID: CommitteeId = CommitteeId([1; 32]);
const BASE_TIMESTAMP_NS: u64 = 1_000_000;

fn test_config() -> Config {
    Config { stale_message_timeout: Duration::from_millis(200), ..Config::default() }
}

struct TestSetup {
    engine: Engine,
    publisher: PeerId,
    _output_rx: mpsc::UnboundedReceiver<EngineOutput>,
    _cmd_tx: mpsc::UnboundedSender<EngineCommand>,
}

fn setup() -> TestSetup {
    let local_keypair = Keypair::generate_ed25519();
    let publisher_keypair = Keypair::generate_ed25519();
    let publisher = PeerId::from(publisher_keypair.public());

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (output_tx, output_rx) = mpsc::unbounded_channel();

    let mut engine = Engine::new(local_keypair, test_config(), cmd_rx, output_tx, None);
    engine
        .register_committee(
            TEST_COMMITTEE_ID,
            vec![
                (engine.local_peer_id, StakingWeight(10), None),
                (publisher, StakingWeight(10), Some(publisher_keypair.public())),
            ],
        )
        .unwrap();

    TestSetup { engine, publisher, _output_rx: output_rx, _cmd_tx: cmd_tx }
}

fn make_unit(publisher: PeerId, nonce: u64, root: MessageRoot) -> PropellerUnit {
    PropellerUnit::new(
        TEST_COMMITTEE_ID,
        publisher,
        root,
        vec![0; 64],
        ShardIndex(0),
        ShardsOfPeer(vec![Shard(vec![1, 2, 3])]),
        MerkleProof { siblings: vec![] },
        nonce,
    )
}

fn message_key(publisher: PeerId, nonce: u64, root: MessageRoot) -> MessageKey {
    MessageKey { committee_id: TEST_COMMITTEE_ID, publisher, nonce, root }
}

fn finalize_message(engine: &mut Engine, publisher: PeerId, nonce: u64, root: MessageRoot) {
    engine.handle_state_manager_message(EventStateManagerToEngine::Finalized {
        committee_id: TEST_COMMITTEE_ID,
        publisher,
        nonce,
        message_root: root,
        shard_status: GoodShardsStatus::SomeGoodShardsReceived,
    });
}

#[tokio::test]
async fn nonce_rejects_unit_with_old_timestamp() {
    let mut s = setup();
    s.engine.peer_nonce.put(s.publisher, BASE_TIMESTAMP_NS);

    // Timestamp equal to nonce — rejected.
    let root_eq = MessageRoot([0u8; 32]);
    s.engine.handle_unit(PeerId::random(), make_unit(s.publisher, BASE_TIMESTAMP_NS, root_eq));
    assert!(
        !s.engine.message_to_unit_tx.contains_key(&message_key(
            s.publisher,
            BASE_TIMESTAMP_NS,
            root_eq
        )),
        "unit with timestamp == nonce must be rejected"
    );

    // Timestamp older than nonce — rejected.
    let root_old = MessageRoot([1u8; 32]);
    s.engine.handle_unit(PeerId::random(), make_unit(s.publisher, BASE_TIMESTAMP_NS - 1, root_old));
    assert!(
        !s.engine.message_to_unit_tx.contains_key(&message_key(
            s.publisher,
            BASE_TIMESTAMP_NS - 1,
            root_old
        )),
        "unit with timestamp < nonce must be rejected"
    );
}

#[tokio::test]
async fn nonce_allows_unit_with_fresh_timestamp() {
    let mut s = setup();
    s.engine.peer_nonce.put(s.publisher, BASE_TIMESTAMP_NS);

    let root = MessageRoot([0u8; 32]);
    s.engine.handle_unit(PeerId::random(), make_unit(s.publisher, BASE_TIMESTAMP_NS + 1, root));

    assert!(
        s.engine.message_to_unit_tx.contains_key(&message_key(
            s.publisher,
            BASE_TIMESTAMP_NS + 1,
            root
        )),
        "unit with timestamp > nonce must be accepted"
    );
}

#[tokio::test]
async fn nonce_updated_on_cache_expiry() {
    tokio::time::pause();
    let mut s = setup();

    // Finalize two messages from the same publisher with different timestamps.
    finalize_message(&mut s.engine, s.publisher, BASE_TIMESTAMP_NS, MessageRoot([1u8; 32]));
    finalize_message(&mut s.engine, s.publisher, BASE_TIMESTAMP_NS + 100, MessageRoot([2u8; 32]));

    // Not yet expired — nonce should not be set.
    assert!(s.engine.peer_nonce.peek(&s.publisher).is_none());

    // Advance past TTL so both messages expire.
    tokio::time::advance(test_config().stale_message_timeout + Duration::from_millis(50)).await;

    // Finalize a third message, which triggers cleanup of the two expired entries.
    finalize_message(&mut s.engine, s.publisher, BASE_TIMESTAMP_NS + 200, MessageRoot([3u8; 32]));

    // Nonce must be the max of the two expired timestamps.
    assert_eq!(
        s.engine.peer_nonce.peek(&s.publisher),
        Some(&(BASE_TIMESTAMP_NS + 100)),
        "nonce must equal the max expired timestamp"
    );
}
