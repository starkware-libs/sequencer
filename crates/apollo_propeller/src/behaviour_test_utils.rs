#![allow(dead_code)]

use std::collections::BTreeMap;
use std::time::Duration;

use libp2p::core::ConnectedPoint;
use libp2p::identity::Keypair;
use libp2p::swarm::behaviour::{ConnectionEstablished, FromSwarm};
use libp2p::swarm::{ConnectionId, NetworkBehaviour, THandlerInEvent, ToSwarm};
use libp2p::{Multiaddr, PeerId};
use starknet_api::staking::StakingWeight;
use tokio::time::error::Elapsed;

use crate::config::Config;
use crate::handler::HandlerIn;
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

pub(crate) struct TestNode {
    pub(crate) behaviour: Behaviour,
    peer_id: PeerId,
    /// Sender end of each connected peer's inbound unit channel, keyed by that peer's id. Lets
    /// `simulate_receive_unit` inject a unit as if it arrived over that peer's connection.
    inbound_senders: BTreeMap<PeerId, futures::channel::mpsc::Sender<PropellerUnit>>,
}

impl TestNode {
    fn new(config: Config) -> Self {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());
        let behaviour = Behaviour::new(keypair, config);

        Self { behaviour, peer_id, inbound_senders: BTreeMap::new() }
    }

    fn simulate_connect_to(&mut self, peer_id: PeerId) {
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
        // Mirror the inbound channel a real handler would register for this connection, retaining
        // the sender so units can be injected from this peer via `simulate_receive_unit`.
        let sender = self.behaviour.register_inbound_channel(peer_id);
        self.inbound_senders.insert(peer_id, sender);
    }

    pub(crate) fn simulate_receive_unit(&mut self, from_peer: PeerId, unit: PropellerUnit) {
        self.inbound_senders
            .get_mut(&from_peer)
            .expect("No inbound channel for peer; simulate_connect_to must be called first")
            .try_send(unit)
            .expect("Inbound unit channel is full or closed");
    }

    async fn next_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<ToSwarm<Event, THandlerInEvent<Behaviour>>, Elapsed> {
        tokio::time::timeout(timeout, futures::future::poll_fn(|cx| self.behaviour.poll(cx))).await
    }

    pub(crate) async fn expect_send_unit(&mut self) -> (PeerId, PropellerUnit) {
        match self.next_with_timeout(EVENT_TIMEOUT).await.unwrap() {
            ToSwarm::NotifyHandler { peer_id, event, .. } => {
                let HandlerIn::SendUnit(unit) = event;
                (peer_id, unit)
            }
            other => panic!("Expected send unit, got {:?}", other),
        }
    }

    pub(crate) async fn expect_message_received(&mut self) -> (PeerId, Vec<u8>) {
        match self.next_with_timeout(EVENT_TIMEOUT).await.unwrap() {
            ToSwarm::GenerateEvent(Event::MessageReceived { publisher, message, .. }) => {
                (publisher, message)
            }
            other => panic!("Expected message received, got {:?}", other),
        }
    }

    pub(crate) async fn expect_no_events(&mut self) {
        if let Ok(event) = self.next_with_timeout(NO_EVENTS_TIMEOUT).await {
            panic!("Expected no events, got {:?}", event);
        }
    }
}

pub(crate) struct TestEnvironment {
    nodes: BTreeMap<PeerId, TestNode>,
}

impl TestEnvironment {
    pub(crate) fn new(num_nodes: usize, config: Config) -> Self {
        let mut nodes = BTreeMap::new();
        for _ in 0..num_nodes {
            let node = TestNode::new(config.clone());
            nodes.insert(node.peer_id, node);
        }
        Self { nodes }
    }

    pub(crate) fn node_mut(&mut self, peer_id: PeerId) -> &mut TestNode {
        self.nodes.get_mut(&peer_id).unwrap()
    }

    pub(crate) fn peer_ids(&self) -> Vec<PeerId> {
        self.nodes.keys().copied().collect()
    }

    // Connect every pair of nodes (full mesh).
    pub(crate) fn simulate_connect_all(&mut self) {
        let peer_ids = self.peer_ids();
        for outer_index in 0..peer_ids.len() {
            for inner_index in (outer_index + 1)..peer_ids.len() {
                let peer_a = peer_ids[outer_index];
                let peer_b = peer_ids[inner_index];
                self.node_mut(peer_a).simulate_connect_to(peer_b);
                self.node_mut(peer_b).simulate_connect_to(peer_a);
            }
        }
    }

    // Register a single committee containing every node in the env, all with equal weight.
    pub(crate) async fn register_committee(&mut self) -> CommitteeId {
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
