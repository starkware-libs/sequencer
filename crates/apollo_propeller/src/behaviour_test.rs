use std::collections::BTreeMap;
use std::time::Duration;

use libp2p::core::ConnectedPoint;
use libp2p::identity::Keypair;
use libp2p::swarm::behaviour::{ConnectionEstablished, FromSwarm};
use libp2p::swarm::{ConnectionId, NetworkBehaviour, THandlerInEvent, ToSwarm};
use libp2p::{Multiaddr, PeerId};
use rstest::rstest;
use starknet_api::staking::StakingWeight;
use tokio::time::error::Elapsed;

use crate::config::Config;
use crate::handler::{HandlerIn, HandlerOut};
use crate::types::{CommitteeId, Event};
use crate::{Behaviour, PropellerUnit};

const EVENT_TIMEOUT: Duration = Duration::from_secs(5);
const NO_EVENTS_TIMEOUT: Duration = Duration::from_millis(50);

fn loopback_dialer_endpoint() -> ConnectedPoint {
    ConnectedPoint::Dialer {
        address: "/ip4/127.0.0.1/tcp/0".parse::<Multiaddr>().unwrap(),
        role_override: libp2p::core::Endpoint::Dialer,
        port_use: libp2p::core::transport::PortUse::New,
    }
}

struct TestNode {
    behaviour: Behaviour,
    peer_id: PeerId,
}

impl TestNode {
    fn new(config: Config) -> Self {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());
        let behaviour = Behaviour::new(keypair, config);

        Self { behaviour, peer_id }
    }

    fn connect_to(&mut self, peer_id: PeerId) {
        let connection_id = ConnectionId::new_unchecked(0);
        let endpoint = loopback_dialer_endpoint();

        let event = FromSwarm::ConnectionEstablished(ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint: &endpoint,
            failed_addresses: &[],
            other_established: 0,
        });

        self.behaviour.on_swarm_event(event);
    }

    fn receive_unit(&mut self, from_peer: PeerId, unit: PropellerUnit) {
        let connection_id = ConnectionId::new_unchecked(0);
        self.behaviour.on_connection_handler_event(
            from_peer,
            connection_id,
            HandlerOut::Unit(unit),
        );
    }

    async fn next_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<ToSwarm<Event, THandlerInEvent<Behaviour>>, Elapsed> {
        tokio::time::timeout(timeout, futures::future::poll_fn(|cx| self.behaviour.poll(cx))).await
    }

    async fn expect_send_unit(&mut self) -> (PeerId, PropellerUnit) {
        match self.next_timeout(EVENT_TIMEOUT).await.unwrap() {
            ToSwarm::NotifyHandler { peer_id, event, .. } => {
                let HandlerIn::SendUnit(unit) = event;
                (peer_id, unit)
            }
            other => panic!("Expected send unit, got {:?}", other),
        }
    }

    async fn expect_message_received(&mut self) -> (PeerId, Vec<u8>) {
        match self.next_timeout(EVENT_TIMEOUT).await.unwrap() {
            ToSwarm::GenerateEvent(Event::MessageReceived { publisher, message, .. }) => {
                (publisher, message)
            }
            other => panic!("Expected message received, got {:?}", other),
        }
    }

    async fn expect_no_events(&mut self) {
        if let Ok(event) = self.next_timeout(NO_EVENTS_TIMEOUT).await {
            panic!("Expected no events, got {:?}", event);
        }
    }
}

struct TestEnvironment {
    nodes: BTreeMap<PeerId, TestNode>,
}

impl TestEnvironment {
    fn new(num_nodes: usize, config: Config) -> Self {
        let mut nodes = BTreeMap::new();
        for _ in 0..num_nodes {
            let node = TestNode::new(config.clone());
            nodes.insert(node.peer_id, node);
        }
        Self { nodes }
    }

    fn node_mut(&mut self, peer_id: PeerId) -> &mut TestNode {
        self.nodes.get_mut(&peer_id).unwrap()
    }

    fn peer_ids(&self) -> Vec<PeerId> {
        self.nodes.keys().copied().collect()
    }

    // Connect every pair of nodes (full mesh).
    fn connect_all(&mut self) {
        let peer_ids = self.peer_ids();
        for outer_index in 0..peer_ids.len() {
            for inner_index in (outer_index + 1)..peer_ids.len() {
                let peer_a = peer_ids[outer_index];
                let peer_b = peer_ids[inner_index];
                self.node_mut(peer_a).connect_to(peer_b);
                self.node_mut(peer_b).connect_to(peer_a);
            }
        }
    }

    // Register a single committee containing every node in the env, all with equal weight.
    async fn register_committee(&mut self) -> CommitteeId {
        let peer_ids = self.peer_ids();
        let committee_id = CommitteeId([0; 32]);
        let peers: Vec<_> = peer_ids.iter().map(|&id| (id, StakingWeight(1))).collect();
        for &peer_id in peer_ids.iter() {
            self.node_mut(peer_id)
                .behaviour
                .register_committee_peers(committee_id, peers.clone())
                .await
                .unwrap()
                .unwrap();
        }
        committee_id
    }
}

#[rstest]
// TODO(AndrewL): make case(1) work. A single-node committee currently has no recipients to drive
// the broadcast/reception flow.
#[case(2)]
#[case(3)]
#[case(4)]
#[case(5)]
#[case(6)]
#[case(7)]
#[case(8)]
#[case(9)]
#[case(10)]
#[tokio::test]
async fn test_broadcast_and_receive(#[case] num_nodes: usize) {
    let config = Config::default();
    let mut env = TestEnvironment::new(num_nodes, config);
    env.connect_all();
    let committee_id = env.register_committee().await;
    let publisher_id = env.peer_ids()[0];
    let message = b"Hello, Propeller!".to_vec();

    env.node_mut(publisher_id)
        .behaviour
        .broadcast(committee_id, message.clone())
        .await
        .unwrap()
        .unwrap();

    // Step 1: Publisher sends initial shards to designated peers (one per recipient).
    let mut initial_shards: BTreeMap<PeerId, PropellerUnit> = BTreeMap::new();
    for _ in 0..(num_nodes - 1) {
        let (recipient, unit) = env.node_mut(publisher_id).expect_send_unit().await;
        assert_eq!(unit.committee_id(), committee_id);
        assert_eq!(unit.publisher(), publisher_id);
        initial_shards.insert(recipient, unit);
    }
    env.node_mut(publisher_id).expect_no_events().await;

    let recipient_ids: Vec<PeerId> = initial_shards.keys().copied().collect();

    // Step 2: For each recipient, deliver its shard from the publisher and collect the gossip
    // it broadcasts to the other recipients. Each recipient gossips to (num_nodes - 2) peers.
    let mut gossip_to_deliver: Vec<(PeerId, PeerId, PropellerUnit)> = Vec::new();
    for &recipient_id in &recipient_ids {
        let unit = initial_shards.get(&recipient_id).unwrap().clone();
        env.node_mut(recipient_id).receive_unit(publisher_id, unit);

        let mut expected_targets: Vec<PeerId> =
            recipient_ids.iter().copied().filter(|&peer| peer != recipient_id).collect();
        for _ in 0..expected_targets.len() {
            let (target, gossip_unit) = env.node_mut(recipient_id).expect_send_unit().await;
            assert_eq!(gossip_unit.committee_id(), committee_id);
            assert_eq!(gossip_unit.publisher(), publisher_id);
            assert!(expected_targets.contains(&target), "Gossip to unexpected peer: {:?}", target);
            expected_targets.retain(|&peer| peer != target);
            gossip_to_deliver.push((recipient_id, target, gossip_unit));
        }
    }

    // Step 3: Deliver every gossipped shard to its target. The sender must match the
    // designated broadcaster of the shard (each recipient broadcasts its own shard).
    for (sender_id, target_id, unit) in gossip_to_deliver {
        env.node_mut(target_id).receive_unit(sender_id, unit);
    }

    // Step 4: Every recipient should reconstruct the message and emit MessageReceived with
    // the original payload.
    for &recipient_id in &recipient_ids {
        let (recv_publisher, recv_message) =
            env.node_mut(recipient_id).expect_message_received().await;
        assert_eq!(recv_publisher, publisher_id);
        assert_eq!(recv_message, message);
    }
}

/// Minimal test that reproduces the shard counting bug in PostReconstruction phase.
///
/// This test creates a 5-node network where:
/// 1. Node receives its first shard and starts reconstruction immediately (since should_build(1) =
///    true)
/// 2. After reconstruction, it needs more shards to reach access threshold (should_receive(2) =
///    true for 5+ nodes)
/// 3. Additional shards arrive via gossip
/// 4. Node should emit MessageReceived event (this failed before the fix)
#[tokio::test]
async fn test_post_reconstruction_shard_counting() {
    // Setup: 5 nodes (1 publisher + 4 recipients)
    let config = Config::default();
    let mut env = TestEnvironment::new(5, config);
    env.connect_all();
    let committee_id = env.register_committee().await;

    let peer_ids = env.peer_ids();
    let publisher_id = peer_ids[0];
    let recipient_id = peer_ids[1]; // The node we're testing

    // Publisher broadcasts a message
    let message = vec![42u8; 1024];
    env.node_mut(publisher_id)
        .behaviour
        .broadcast(committee_id, message.clone())
        .await
        .unwrap()
        .unwrap();

    // Collect initial broadcast from publisher (4 shards to 4 recipients)
    let mut initial_shards = BTreeMap::new();
    for _ in 0..4 {
        let (peer, unit) = env.node_mut(publisher_id).expect_send_unit().await;
        initial_shards.insert(peer, unit);
    }

    // Recipient receives its assigned shard (index 0) from publisher
    // This triggers immediate reconstruction since should_build(1) = true
    let recipient_shard = initial_shards.get(&recipient_id).unwrap().clone();
    env.node_mut(recipient_id).receive_unit(publisher_id, recipient_shard.clone());

    // Recipient gossips its shard to other recipients
    for _ in 0..3 {
        let (_peer, _unit) = env.node_mut(recipient_id).expect_send_unit().await;
    }

    // Now recipient receives one additional shard from another recipient.
    // This is the critical moment: before the fix, the additional_shards counter was not
    // incremented.
    // The sender must be the designated broadcaster for the shard (validate_origin checks this),
    // so we pick a shard that was sent to a peer other than recipient_id and deliver it from
    // that peer.
    let (&another_recipient_id, another_shard) =
        initial_shards.iter().find(|(&peer, _)| peer != recipient_id).unwrap();
    let another_shard = another_shard.clone();
    env.node_mut(recipient_id).receive_unit(another_recipient_id, another_shard);

    // After receiving 2 shards total (1 at reconstruction + 1 additional),
    // should_receive(2) = true, so the node should emit MessageReceived
    let (recv_publisher, recv_message) = env.node_mut(recipient_id).expect_message_received().await;
    assert_eq!(recv_publisher, publisher_id);
    assert_eq!(recv_message, message);
}
