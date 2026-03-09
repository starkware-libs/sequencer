use std::time::{SystemTime, UNIX_EPOCH};

use libp2p::identity::Keypair;
use libp2p::PeerId;
use tokio::sync::mpsc;

use super::*;
use crate::types::{MessageRoot, ShardIndex};
use crate::MerkleTree;

const TEST_CHANNEL: Channel = Channel(1);
const SHARD_DATA: [u8; 3] = [1, 2, 3];

fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is set")
        .as_nanos()
        .try_into()
        .expect("timestamp in nanos since UNIX_EPOCH should fit in u64, until year 2554")
}

fn setup_engine() -> (Engine, mpsc::UnboundedReceiver<EngineOutput>) {
    let keypair = Keypair::generate_ed25519();
    let config = Config::default();
    let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (output_tx, output_rx) = mpsc::unbounded_channel();

    let engine = Engine::new(keypair, config, cmd_rx, output_tx, None);
    (engine, output_rx)
}

fn make_unit(publisher: PeerId, timestamp_ns: u64) -> PropellerUnit {
    let merkle_tree = MerkleTree::new(&vec![SHARD_DATA.to_vec(); 4]);
    let root = MessageRoot(merkle_tree.root().unwrap());
    PropellerUnit::new(
        TEST_CHANNEL,
        publisher,
        root,
        vec![0u8; 64],
        ShardIndex(0),
        SHARD_DATA.to_vec(),
        merkle_tree.prove(0).unwrap(),
        timestamp_ns,
    )
}

#[tokio::test]
async fn test_fresh_unit_is_not_dropped() {
    let (mut engine, _output_rx) = setup_engine();

    let publisher_keypair = Keypair::generate_ed25519();
    let publisher = PeerId::from(publisher_keypair.public());

    engine
        .register_channel(
            TEST_CHANNEL,
            vec![
                (engine.local_peer_id, 10, None),
                (publisher, 10, Some(publisher_keypair.public())),
            ],
        )
        .unwrap();

    let fresh_timestamp_ns = now_nanos();
    let unit = make_unit(publisher, fresh_timestamp_ns);

    engine.handle_unit(publisher, unit);

    assert!(!engine.message_to_unit_tx.is_empty(), "Fresh unit should reach the message processor");
}

#[test]
fn test_stale_unit_is_dropped() {
    let (mut engine, _output_rx) = setup_engine();

    let publisher_keypair = Keypair::generate_ed25519();
    let publisher = PeerId::from(publisher_keypair.public());

    engine
        .register_channel(
            TEST_CHANNEL,
            vec![
                (engine.local_peer_id, 10, None),
                (publisher, 10, Some(publisher_keypair.public())),
            ],
        )
        .unwrap();

    let threshold_ns: u64 = engine
        .config
        .stale_message_timeout
        .as_nanos()
        .try_into()
        .expect("stale threshold in nanos should fit in u64");
    let stale_timestamp_ns = now_nanos() - threshold_ns - 1;
    let unit = make_unit(publisher, stale_timestamp_ns);
    let sender = PeerId::random();

    engine.handle_unit(sender, unit);

    assert!(
        engine.message_to_unit_tx.is_empty(),
        "Stale unit should be dropped before spawning a message processor"
    );
}
