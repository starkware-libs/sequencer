use std::time::Duration;

use apollo_propeller::types::{Channel, Event};
use apollo_propeller::{Behaviour, Config};
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, Swarm};
use libp2p_swarm_test::SwarmExt;
use rstest::rstest;
use tokio::sync::mpsc;

const TIMEOUT: Duration = Duration::from_secs(5);

async fn create_propeller_swarm() -> Swarm<Behaviour> {
    let mut swarm =
        Swarm::new_ephemeral_tokio(|keypair| Behaviour::new(keypair.clone(), Config::default()));
    swarm.listen().with_memory_addr_external().await;
    swarm
}

async fn setup_connected_nodes(n: usize) -> Vec<Swarm<Behaviour>> {
    let mut swarms = Vec::new();
    for _ in 0..n {
        swarms.push(create_propeller_swarm().await);
    }

    for i in 0..n {
        for j in (i + 1)..n {
            let (a, b) = swarms.split_at_mut(j);
            a[i].connect(&mut b[0]).await;
        }
    }

    swarms
}

async fn register_channel_on_all(swarms: &mut [Swarm<Behaviour>], channel: Channel) {
    let peer_ids: Vec<PeerId> = swarms.iter().map(|s| *s.local_peer_id()).collect();
    let peers: Vec<(PeerId, u64)> = peer_ids.iter().map(|&id| (id, 1)).collect();
    for swarm in swarms.iter_mut() {
        swarm
            .behaviour_mut()
            .register_channel_peers(channel, peers.clone())
            .await
            .expect("Failed to register channel");
    }
}

/// Spawns swarm event loops for all swarms. Returns a single merged receiver for all
/// `MessageReceived` events as `(publisher, message)`.
fn spawn_swarm_drivers(
    swarms: Vec<Swarm<Behaviour>>,
) -> mpsc::UnboundedReceiver<(PeerId, Vec<u8>)> {
    let (tx, rx) = mpsc::unbounded_channel();
    for mut swarm in swarms {
        let tx = tx.clone();
        tokio::spawn(async move {
            loop {
                if let SwarmEvent::Behaviour(Event::MessageReceived {
                    publisher, message, ..
                }) = swarm.select_next_some().await
                {
                    let _ = tx.send((publisher, message));
                }
            }
        });
    }
    rx
}

/// Collects exactly `expected` messages from the receiver within TIMEOUT.
async fn collect_messages(
    rx: &mut mpsc::UnboundedReceiver<(PeerId, Vec<u8>)>,
    expected: usize,
) -> Vec<(PeerId, Vec<u8>)> {
    let mut received = Vec::with_capacity(expected);
    let result = tokio::time::timeout(TIMEOUT, async {
        while received.len() < expected {
            let msg = rx.recv().await.expect("Swarm driver dropped unexpectedly");
            received.push(msg);
        }
    })
    .await;
    assert!(result.is_ok(), "Timed out: received {}/{expected} messages", received.len());
    received
}

#[rstest]
// TODO(AndrewL): make the 1-node case pass.
#[case(2)]
#[case(3)]
#[case(5)]
#[case(10)]
#[tokio::test]
async fn e2e_broadcast_single_message(
    #[case] num_nodes: usize,
    #[values(17, 4096, 65536)] message_size: usize,
    #[values(1, 2)] num_publishers: usize,
) {
    let mut swarms = setup_connected_nodes(num_nodes).await;
    let channel = Channel(0);
    register_channel_on_all(&mut swarms, channel).await;

    let mut publisher_ids = Vec::new();
    let mut messages = Vec::new();

    for (pub_idx, swarm) in swarms.iter_mut().enumerate().take(num_publishers) {
        let publisher_id = *swarm.local_peer_id();
        let message: Vec<u8> =
            (0..message_size).map(|i| u8::try_from((i + pub_idx) % 256).unwrap()).collect();
        swarm
            .behaviour_mut()
            .broadcast(channel, message.clone())
            .await
            .expect("Broadcast should succeed");
        publisher_ids.push(publisher_id);
        messages.push(message);
    }

    let expected_total = num_publishers * (num_nodes - 1);
    let mut rx = spawn_swarm_drivers(swarms);
    let received = collect_messages(&mut rx, expected_total).await;

    for (publisher, msg) in &received {
        let pub_idx = publisher_ids
            .iter()
            .position(|id| id == publisher)
            .expect("Received message from unexpected publisher");
        assert_eq!(*msg, messages[pub_idx]);
    }
}
