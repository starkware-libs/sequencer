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

#[expect(dead_code)]
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

#[expect(dead_code)]
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
