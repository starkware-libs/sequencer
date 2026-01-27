use std::collections::HashMap;
use std::time::Duration;

use libp2p::core::ConnectedPoint;
use libp2p::identity::Keypair;
use libp2p::swarm::behaviour::{ConnectionClosed, ConnectionEstablished, FromSwarm};
use libp2p::swarm::{ConnectionId, NetworkBehaviour, THandlerInEvent, ToSwarm};
use libp2p::{Multiaddr, PeerId};
use rstest::rstest;
use tokio::time::error::Elapsed;

use crate::config::Config;
use crate::handler::HandlerOut;
use crate::types::{Channel, Event};
use crate::{Behaviour, PropellerUnit};

const TIMEOUT: Duration = Duration::from_secs(5);

#[allow(dead_code)]
struct TestNode {
    behaviour: Behaviour,
    peer_id: PeerId,
    keypair: Keypair,
}

#[allow(dead_code)]
impl TestNode {
    fn new(config: Config) -> Self {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());
        let behaviour = Behaviour::new(keypair.clone(), config.clone());

        Self { behaviour, peer_id, keypair }
    }

    /// Simulate a connection being established with another peer.
    fn connect_to(&mut self, peer_id: PeerId) {
        let connection_id = ConnectionId::new_unchecked(0);
        let endpoint = ConnectedPoint::Dialer {
            address: "/ip4/127.0.0.1/tcp/0".parse::<Multiaddr>().unwrap(),
            role_override: libp2p::core::Endpoint::Dialer,
            port_use: libp2p::core::transport::PortUse::New,
        };

        let event = FromSwarm::ConnectionEstablished(ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint: &endpoint,
            failed_addresses: &[],
            other_established: 0,
        });

        self.behaviour.on_swarm_event(event);
    }

    /// Simulate a disconnection from another peer.
    fn disconnect_from(&mut self, peer_id: PeerId) {
        let connection_id = ConnectionId::new_unchecked(0);
        let endpoint = ConnectedPoint::Dialer {
            address: "/ip4/127.0.0.1/tcp/0".parse::<Multiaddr>().unwrap(),
            role_override: libp2p::core::Endpoint::Dialer,
            port_use: libp2p::core::transport::PortUse::New,
        };

        let event = FromSwarm::ConnectionClosed(ConnectionClosed {
            peer_id,
            connection_id,
            endpoint: &endpoint,
            remaining_established: 0,
            cause: None,
        });

        self.behaviour.on_swarm_event(event);
    }

    /// Deliver a unit from another peer to this node.
    fn receive_unit(&mut self, from_peer: PeerId, unit: PropellerUnit) {
        let connection_id = ConnectionId::new_unchecked(0);
        self.behaviour.on_connection_handler_event(
            from_peer,
            connection_id,
            HandlerOut::Unit(unit),
        );
    }

    /// Try to get the next event with a custom timeout. Returns None on timeout.
    async fn next_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<ToSwarm<Event, THandlerInEvent<Behaviour>>, Elapsed> {
        tokio::time::timeout(timeout, futures::future::poll_fn(|cx| self.behaviour.poll(cx))).await
    }

    /// Expect a specific unit to be sent to a peer.
    async fn expect_send_unit(&mut self) -> (PeerId, PropellerUnit) {
        match self.next_timeout(TIMEOUT).await.unwrap() {
            ToSwarm::NotifyHandler { peer_id, event, .. } => {
                let crate::handler::HandlerIn::SendUnit(unit) = event;
                (peer_id, unit)
            }
            other => panic!("Expected send unit, got {:?}", other),
        }
    }

    /// Expect a message received event.
    async fn expect_message_received(&mut self) -> (PeerId, Vec<u8>) {
        match self.next_timeout(TIMEOUT).await.unwrap() {
            ToSwarm::GenerateEvent(Event::MessageReceived { publisher, message, .. }) => {
                (publisher, message)
            }
            other => panic!("Expected message received, got {:?}", other),
        }
    }

    /// Check if there are no immediate events (with a short timeout).
    async fn expect_no_events(&mut self) {
        if let Ok(event) = self.next_timeout(Duration::from_millis(50)).await {
            panic!("Expected no events, got {:?}", event);
        }
    }
}

/// Test environment with multiple nodes.
#[allow(dead_code)]
struct TestEnv {
    nodes: HashMap<PeerId, TestNode>,
}

#[allow(dead_code)]
impl TestEnv {
    fn new(num_nodes: usize, config: Config) -> Self {
        let mut nodes = HashMap::new();
        for _ in 0..num_nodes {
            let node = TestNode::new(config.clone());
            nodes.insert(node.peer_id, node);
        }
        Self { nodes }
    }

    fn node(&self, peer_id: PeerId) -> &TestNode {
        self.nodes.get(&peer_id).unwrap()
    }

    fn node_mut(&mut self, peer_id: PeerId) -> &mut TestNode {
        self.nodes.get_mut(&peer_id).unwrap()
    }

    fn peer_ids(&self) -> Vec<PeerId> {
        self.nodes.keys().copied().collect()
    }

    /// Connect all nodes in a full mesh.
    fn connect_all(&mut self) {
        let peer_ids = self.peer_ids();
        for i in 0..peer_ids.len() {
            for j in (i + 1)..peer_ids.len() {
                let peer1 = peer_ids[i];
                let peer2 = peer_ids[j];
                self.node_mut(peer1).connect_to(peer2);
                self.node_mut(peer2).connect_to(peer1);
            }
        }
    }

    /// Register a channel with specific peer weights and public keys.
    async fn register_channel(&mut self) -> Channel {
        let peer_ids = self.peer_ids();
        let channel = Channel(0);
        let peers: Vec<(PeerId, u64)> = peer_ids.iter().map(|&id| (id, 1)).collect();
        for &(peer_id, _) in peers.iter() {
            self.node_mut(peer_id)
                .behaviour
                .register_channel_peers(channel, peers.clone())
                .await
                .expect("Failed to register channel");
        }
        channel
    }
}

#[rstest]
// #[case(1)]
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
async fn test_broadcast_and_receive(#[case] num_nodes: usize, #[values(0, 1, 2)] num_steps: usize) {
    // Setup: Create environment with N nodes
    let config = Config::default();
    let mut env = TestEnv::new(num_nodes, config);
    env.connect_all();
    let channel = env.register_channel().await;
    let publisher_id = env.peer_ids()[0];
    let message = b"Hello, Propeller!".to_vec();

    // Publisher broadcasts the message
    let message_root = env
        .node_mut(publisher_id)
        .behaviour
        .broadcast(channel, message.clone())
        .await
        .expect("Broadcast should succeed");

    if num_steps == 0 {
        return;
    }

    // Step 1: Publisher sends initial shards to designated peers (waiting for each)
    let mut initial_broadcast = Vec::new();
    for _ in 0..(num_nodes - 1) {
        let (recipient, unit) = env.node_mut(publisher_id).expect_send_unit().await;
        assert_eq!(unit.channel(), channel);
        assert_eq!(unit.publisher(), publisher_id);
        assert_eq!(unit.root(), message_root);
        initial_broadcast.push((recipient, unit));
    }
    env.node_mut(publisher_id).expect_no_events().await;

    if num_steps == 1 {
        return;
    }

    for (recipient, unit) in initial_broadcast.clone() {
        env.node_mut(recipient).receive_unit(publisher_id, unit);
    }

    if num_steps == 2 || num_nodes <= 3 {
        return;
    }

    for (recipient_1, _) in initial_broadcast {
        let mut peers = env.peer_ids()[1..].to_vec();
        peers.retain(|&peer| peer != recipient_1);
        while !peers.is_empty() {
            let (peer_to_send, unit) = env.node_mut(recipient_1).expect_send_unit().await;
            assert_eq!(unit.channel(), channel);
            assert_eq!(unit.publisher(), publisher_id);
            assert_eq!(unit.root(), message_root);
            assert!(peers.contains(&peer_to_send));
            peers.retain(|&peer| peer != peer_to_send);
        }
        env.node_mut(recipient_1).expect_no_events().await;
    }
    unreachable!();
}
