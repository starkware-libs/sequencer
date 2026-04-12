// TODO(andrew): test non-nonce scenarios.
use std::time::Duration;

use libp2p::identity::Keypair;
use libp2p::PeerId;
use starknet_api::staking::StakingWeight;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::engine::{Engine, EngineCommand, EngineOutput, MessageKey};
use crate::message_processor::EventStateManagerToEngine;
use crate::types::{CommitteeId, MessageRoot};
use crate::{MerkleProof, PropellerUnit, Shard, ShardsOfPeer, UnitIndex};

const TEST_COMMITTEE_ID: CommitteeId = CommitteeId([1; 32]);
const BASE_NONCE: u64 = 1_000_000;

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
        UnitIndex(0),
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
        had_good_units: true,
    });
}

#[tokio::test]
async fn reject_unit_of_new_message_with_old_nonce() {
    let mut s = setup();
    s.engine.peer_nonce.put(s.publisher, BASE_NONCE);

    // Nonce earlier than BASE_NONCE are rejected.
    let root_old = MessageRoot([1u8; 32]);
    s.engine.handle_unit(PeerId::random(), make_unit(s.publisher, BASE_NONCE - 1, root_old));
    assert!(
        !s.engine.message_to_unit_tx.contains_key(&message_key(
            s.publisher,
            BASE_NONCE - 1,
            root_old
        )),
        "unit with nonce < BASE_NONCE must be rejected"
    );

    // Nonce equal to BASE_NONCE: rejected.
    let root_eq = MessageRoot([0u8; 32]);
    s.engine.handle_unit(PeerId::random(), make_unit(s.publisher, BASE_NONCE, root_eq));
    assert!(
        !s.engine.message_to_unit_tx.contains_key(&message_key(s.publisher, BASE_NONCE, root_eq)),
        "unit with nonce == BASE_NONCE must be rejected"
    );
}

#[tokio::test]
async fn allow_unit_of_new_message_with_fresh_nonce() {
    let mut s = setup();
    s.engine.peer_nonce.put(s.publisher, BASE_NONCE);

    let root = MessageRoot([0u8; 32]);
    s.engine.handle_unit(PeerId::random(), make_unit(s.publisher, BASE_NONCE + 1, root));

    assert!(
        s.engine.message_to_unit_tx.contains_key(&message_key(s.publisher, BASE_NONCE + 1, root)),
        "unit with nonce > BASE_NONCE must be accepted"
    );
}

#[tokio::test]
async fn nonce_updated_on_cache_expiry() {
    tokio::time::pause();
    let mut s = setup();

    // Finalize two messages from the same publisher with different nonces.
    finalize_message(&mut s.engine, s.publisher, BASE_NONCE + 100, MessageRoot([2u8; 32]));
    tokio::time::advance(Duration::from_millis(100)).await;
    finalize_message(&mut s.engine, s.publisher, BASE_NONCE, MessageRoot([1u8; 32]));

    // Before expiry: no nonce is cached (effective nonce = 0), so a unit with any nonce should be
    // accepted.
    let root_before = MessageRoot([10u8; 32]);
    s.engine.handle_unit(PeerId::random(), make_unit(s.publisher, BASE_NONCE - 50, root_before));
    assert!(
        s.engine.message_to_unit_tx.contains_key(&message_key(
            s.publisher,
            BASE_NONCE - 50,
            root_before
        )),
        "unit must be accepted when nonce cache has no entry for this publisher"
    );

    // Advance past TTL so both messages expire, then trigger cleanup via a third finalization.
    tokio::time::advance(test_config().stale_message_timeout + Duration::from_millis(50)).await;
    finalize_message(&mut s.engine, s.publisher, BASE_NONCE + 200, MessageRoot([3u8; 32]));

    // After expiry: nonce advances to max(BASE_NONCE, BASE_NONCE+100) = BASE_NONCE+100.
    // A unit with nonce <= BASE_NONCE+100 must now be rejected.
    let root_stale = MessageRoot([11u8; 32]);
    s.engine.handle_unit(PeerId::random(), make_unit(s.publisher, BASE_NONCE + 50, root_stale));
    assert!(
        !s.engine.message_to_unit_tx.contains_key(&message_key(
            s.publisher,
            BASE_NONCE + 50,
            root_stale
        )),
        "unit with nonce <= max expired nonce must be rejected after cache expiry"
    );

    // A unit with nonce strictly above BASE_NONCE+100 must still be accepted.
    let root_fresh = MessageRoot([12u8; 32]);
    s.engine.handle_unit(PeerId::random(), make_unit(s.publisher, BASE_NONCE + 101, root_fresh));
    assert!(
        s.engine.message_to_unit_tx.contains_key(&message_key(
            s.publisher,
            BASE_NONCE + 101,
            root_fresh
        )),
        "unit with nonce > max expired nonce must be accepted after cache expiry"
    );
}
