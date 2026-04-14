use std::collections::{HashMap, HashSet};
use std::iter;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use apollo_network::discovery::DiscoveryConfig;
use apollo_network::gossipsub_impl::Topic;
use apollo_network::misconduct_score::MisconductScore;
use apollo_network::mixed_behaviour::{self, MixedBehaviour};
use apollo_network::network_manager::swarm_trait::{Event, SwarmTrait};
use apollo_network::network_manager::GenericNetworkManager;
use apollo_network::peer_manager::PeerManagerConfig;
use apollo_network::prune_dead_connections::{DEFAULT_PING_INTERVAL, DEFAULT_PING_TIMEOUT};
use apollo_network::sqmr::behaviour::SessionIdNotFoundError;
use apollo_network::sqmr::{self, InboundSessionId, OutboundSessionId, SessionId};
use apollo_network::Bytes;
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use libp2p::core::multiaddr::Protocol;
use libp2p::gossipsub::{MessageId, PublishError, SubscriptionError, TopicHash};
use libp2p::swarm::{DialError, SwarmEvent};
use libp2p::{Multiaddr, PeerId, StreamProtocol, Swarm};
use libp2p_swarm_test::SwarmExt;
use starknet_api::core::ChainId;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::{self, Instant, MissedTickBehavior};

const MESSAGE_METADATA_BUFFER_SIZE: usize = 100;

/// Small-scale variant that runs in CI.
#[tokio::test]
async fn five_peers_discover_each_other_when_given_peer_ids() {
    run_discovery_test(5, Duration::from_secs(15)).await;
}

/// Validates that 100 peers can discover each other through a single bootstrap node.
/// This is a long-running test (~30s in release mode) and is ignored by default.
/// Run intentionally with:
/// `cargo test -p apollo_network --release e2e_discovery -- --ignored --no-capture`
#[ignore]
#[tokio::test]
async fn hundred_peers_discover_each_other_when_given_peer_ids() {
    run_discovery_test(100, Duration::from_secs(30)).await;
}

async fn run_discovery_test(num_peers: usize, timeout: Duration) {
    // Create bootstrap node and get its address before wrapping in NetworkManager.
    let mut bootstrap_swarm = create_swarm(None);
    let bootstrap_address = get_listen_address(&mut bootstrap_swarm).await;
    let bootstrap_peer_id = *bootstrap_swarm.local_peer_id();
    let bootstrap_multiaddr = bootstrap_address.with_p2p(bootstrap_peer_id).unwrap();

    // Create peer swarms without polling them to avoid discarding initial RequestDial events
    // from bootstrapping before the network manager is ready to route them.
    let mut swarms: Vec<Swarm<MixedBehaviour>> =
        (0..num_peers).map(|_| create_swarm(Some(bootstrap_multiaddr.clone()))).collect();

    // The bootstrap node is excluded from the full-mesh connectivity check since it serves
    // only as an initial discovery relay.
    let non_bootstrap_peer_ids: HashSet<PeerId> =
        swarms.iter().map(|s| *s.local_peer_id()).collect();

    // Set allowed peers on all swarms so the whitelist permits connections.
    let all_peer_ids: HashSet<PeerId> =
        iter::once(bootstrap_peer_id).chain(non_bootstrap_peer_ids.iter().copied()).collect();
    bootstrap_swarm.behaviour_mut().peer_access_control.set_allowed_peers(all_peer_ids.clone());
    for swarm in &mut swarms {
        swarm.behaviour_mut().peer_access_control.set_allowed_peers(all_peer_ids.clone());
    }

    let bootstrap_network_manager = create_network_manager(bootstrap_swarm);

    // Tell each swarm's discovery behaviour about all peers before wrapping.
    for swarm in &mut swarms {
        let excluding_self: HashSet<PeerId> = non_bootstrap_peer_ids
            .iter()
            .copied()
            .filter(|id| id != swarm.local_peer_id())
            .collect();
        swarm.behaviour_mut().discovery.as_mut().unwrap().set_target_peers(excluding_self);
    }

    // Channels for reporting connection/disconnection events from wrapper swarms. We'll pass
    // the senders to each reporting swarm and pass the receivers to the tracker.
    let (connection_sender, connection_receiver) = mpsc::unbounded_channel::<(PeerId, PeerId)>();
    let (disconnection_sender, disconnection_receiver) =
        mpsc::unbounded_channel::<(PeerId, PeerId)>();
    let (bootstrap_sender, bootstrap_receiver) = mpsc::unbounded_channel::<PeerId>();

    // Spawn the bootstrap network manager. The task handle is intentionally dropped because the
    // task runs for the test's lifetime and is cleaned up when the tokio runtime shuts down.
    tokio::spawn(async move {
        let _ = bootstrap_network_manager.run().await;
    });

    // Wrap each peer swarm in a ConnectionReportingSwarm, then in a NetworkManager, and spawn.
    // Task handles are intentionally dropped, same as the bootstrap task above.
    for swarm in swarms {
        let local_peer_id = *swarm.local_peer_id();
        let reporting_swarm = ConnectionReportingSwarm::new(
            swarm,
            local_peer_id,
            bootstrap_peer_id,
            non_bootstrap_peer_ids.clone(),
            connection_sender.clone(),
            disconnection_sender.clone(),
            bootstrap_sender.clone(),
        );
        let network_manager = create_reporting_network_manager(reporting_swarm);

        tokio::spawn(async move {
            let _ = network_manager.run().await;
        });
    }
    drop(connection_sender);
    drop(disconnection_sender);
    drop(bootstrap_sender);

    let mut tracker = ConnectionsTracker::new(
        &non_bootstrap_peer_ids,
        connection_receiver,
        disconnection_receiver,
        bootstrap_receiver,
    );
    if time::timeout(timeout, tracker.run_until_full_mesh()).await.is_err() {
        tracker.panic_with_diagnostics("Timed out");
    }
}

/// Tracks current peer-to-peer and bootstrap connections for progress reporting.
struct ConnectionsTracker {
    connections: HashMap<PeerId, HashSet<PeerId>>,
    bootstrap_connections: HashSet<PeerId>,
    ordered_peer_ids: Vec<PeerId>,
    num_peers: usize,
    connection_receiver: UnboundedReceiver<(PeerId, PeerId)>,
    disconnection_receiver: UnboundedReceiver<(PeerId, PeerId)>,
    bootstrap_receiver: UnboundedReceiver<PeerId>,
}

impl ConnectionsTracker {
    fn new(
        peer_ids: &HashSet<PeerId>,
        connection_receiver: UnboundedReceiver<(PeerId, PeerId)>,
        disconnection_receiver: UnboundedReceiver<(PeerId, PeerId)>,
        bootstrap_receiver: UnboundedReceiver<PeerId>,
    ) -> Self {
        let num_peers = peer_ids.len();
        let connections = peer_ids.iter().map(|id| (*id, HashSet::new())).collect();
        let mut ordered_peer_ids: Vec<PeerId> = peer_ids.iter().copied().collect();
        ordered_peer_ids.sort_by_key(|id| id.to_string());
        Self {
            connections,
            bootstrap_connections: HashSet::new(),
            ordered_peer_ids,
            num_peers,
            connection_receiver,
            disconnection_receiver,
            bootstrap_receiver,
        }
    }

    /// Collects connections until full mesh, printing progress every second.
    async fn run_until_full_mesh(&mut self) {
        let total_expected = self.num_peers * (self.num_peers - 1);
        let mut tick = time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let start = Instant::now();

        loop {
            tokio::select! {
                msg = self.connection_receiver.recv() => {
                    match msg {
                        Some((from, to)) => {
                            self.add_connection(from, to);
                            if self.total_connections() >= total_expected {
                                self.print_progress(start, total_expected);
                                return;
                            }
                        }
                        None => self.panic_with_diagnostics("Channels closed prematurely"),
                    }
                }
                msg = self.disconnection_receiver.recv() => {
                    if let Some((from, to)) = msg {
                        self.remove_connection(from, to);
                    }
                }
                msg = self.bootstrap_receiver.recv() => {
                    if let Some(peer_id) = msg {
                        self.add_bootstrap_connection(peer_id);
                    }
                }
                _ = tick.tick() => {
                    self.print_progress(start, total_expected);
                }
            }
        }
    }

    fn panic_with_diagnostics(&self, reason: &str) -> ! {
        let total_expected = self.num_peers * (self.num_peers - 1);
        let total = self.total_connections();
        // Test failed. Printing information before panicking.
        let mut connections_per_peer: Vec<_> =
            self.connections.iter().map(|(id, peers)| (peers.len(), id)).collect();
        connections_per_peer.sort();
        eprintln!("\nPer-peer connection counts (sorted):");
        for (num_connections, peer_id) in &connections_per_peer {
            eprintln!("  {num_connections:>3}/{}  {peer_id}", self.num_peers - 1);
        }
        panic!(
            "{reason} waiting for full connectivity: {total}/{total_expected} connections \
             established",
        );
    }

    /// Safe to unwrap: `from` is always `local_peer_id` of a peer swarm, which is guaranteed
    /// to be in `peer_ids` (and therefore in `self.connections`) by construction.
    fn add_connection(&mut self, from: PeerId, to: PeerId) {
        self.connections.get_mut(&from).unwrap().insert(to);
    }

    fn remove_connection(&mut self, from: PeerId, to: PeerId) {
        self.connections.get_mut(&from).unwrap().remove(&to);
    }

    fn add_bootstrap_connection(&mut self, peer_id: PeerId) {
        self.bootstrap_connections.insert(peer_id);
    }

    fn total_connections(&self) -> usize {
        self.connections.values().map(|s| s.len()).sum()
    }

    fn print_progress(&self, start: Instant, total_expected: usize) {
        let total = self.total_connections();
        let bootstrap_count = self.bootstrap_connections.len();
        let per_peer: String = self
            .ordered_peer_ids
            .iter()
            .map(|id| self.connections[id].len().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let elapsed = start.elapsed().as_secs_f64();
        println!(
            "{elapsed:.1}s total_connections: {total}/{total_expected} bootstrap: \
             {bootstrap_count}/{} per_peer: {per_peer}",
            self.num_peers
        );
    }
}

/// A wrapper around `Swarm<MixedBehaviour>` that reports connection and disconnection events
/// via channels, allowing tests to observe connectivity without modifying NetworkManager.
struct ConnectionReportingSwarm {
    inner: Swarm<MixedBehaviour>,
    local_peer_id: PeerId,
    bootstrap_peer_id: PeerId,
    known_peer_ids: HashSet<PeerId>,
    connection_sender: UnboundedSender<(PeerId, PeerId)>,
    disconnection_sender: UnboundedSender<(PeerId, PeerId)>,
    bootstrap_sender: UnboundedSender<PeerId>,
}

impl ConnectionReportingSwarm {
    fn new(
        inner: Swarm<MixedBehaviour>,
        local_peer_id: PeerId,
        bootstrap_peer_id: PeerId,
        known_peer_ids: HashSet<PeerId>,
        connection_sender: UnboundedSender<(PeerId, PeerId)>,
        disconnection_sender: UnboundedSender<(PeerId, PeerId)>,
        bootstrap_sender: UnboundedSender<PeerId>,
    ) -> Self {
        Self {
            inner,
            local_peer_id,
            bootstrap_peer_id,
            known_peer_ids,
            connection_sender,
            disconnection_sender,
            bootstrap_sender,
        }
    }
}

impl futures::Stream for ConnectionReportingSwarm {
    type Item = Event;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let poll = Pin::new(&mut self.inner).poll_next(cx);
        match &poll {
            Poll::Ready(Some(SwarmEvent::ConnectionEstablished { peer_id, .. })) => {
                let connected_peer_id = *peer_id;
                if connected_peer_id == self.bootstrap_peer_id {
                    let _ = self.bootstrap_sender.send(self.local_peer_id);
                } else if self.known_peer_ids.contains(&connected_peer_id) {
                    let _ = self.connection_sender.send((self.local_peer_id, connected_peer_id));
                }
            }
            Poll::Ready(Some(SwarmEvent::ConnectionClosed {
                peer_id, num_established: 0, ..
            })) => {
                let disconnected_peer_id = *peer_id;
                if self.known_peer_ids.contains(&disconnected_peer_id) {
                    let _ =
                        self.disconnection_sender.send((self.local_peer_id, disconnected_peer_id));
                }
            }
            _ => {}
        }
        poll
    }
}

impl Unpin for ConnectionReportingSwarm {}

impl SwarmTrait for ConnectionReportingSwarm {
    fn send_response(
        &mut self,
        response: Vec<u8>,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        self.inner.send_response(response, inbound_session_id)
    }

    fn send_query(&mut self, query: Vec<u8>, protocol: StreamProtocol) -> OutboundSessionId {
        self.inner.send_query(query, protocol)
    }

    fn dial(&mut self, peer_multiaddr: Multiaddr) -> Result<(), DialError> {
        SwarmTrait::dial(&mut self.inner, peer_multiaddr)
    }

    fn close_inbound_session(
        &mut self,
        session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        self.inner.close_inbound_session(session_id)
    }

    fn behaviour_mut(&mut self) -> &mut mixed_behaviour::MixedBehaviour {
        SwarmTrait::behaviour_mut(&mut self.inner)
    }

    fn get_peer_id_from_session_id(
        &self,
        session_id: SessionId,
    ) -> Result<PeerId, SessionIdNotFoundError> {
        self.inner.get_peer_id_from_session_id(session_id)
    }

    fn add_external_address(&mut self, address: Multiaddr) {
        SwarmTrait::add_external_address(&mut self.inner, address)
    }

    fn subscribe_to_topic(&mut self, topic: &Topic) -> Result<(), SubscriptionError> {
        self.inner.subscribe_to_topic(topic)
    }

    fn broadcast_message(
        &mut self,
        message: Bytes,
        topic_hash: TopicHash,
    ) -> Result<MessageId, PublishError> {
        self.inner.broadcast_message(message, topic_hash)
    }

    fn report_peer_as_malicious(&mut self, peer_id: PeerId, misconduct_score: MisconductScore) {
        self.inner.report_peer_as_malicious(peer_id, misconduct_score)
    }

    fn add_new_supported_inbound_protocol(&mut self, protocol_name: StreamProtocol) {
        self.inner.add_new_supported_inbound_protocol(protocol_name)
    }

    fn continue_propagation(&mut self, message_metadata: BroadcastedMessageMetadata) {
        self.inner.continue_propagation(message_metadata)
    }
}

fn create_swarm(bootstrap_peer_multiaddr: Option<Multiaddr>) -> Swarm<MixedBehaviour> {
    let event_metrics = None;
    let latency_metrics = None;
    let node_version = None;
    let mut swarm = Swarm::new_ephemeral_tokio(|keypair| {
        MixedBehaviour::new(
            sqmr::Config::default(),
            DiscoveryConfig::default(),
            PeerManagerConfig::default(),
            event_metrics,
            latency_metrics,
            keypair.clone(),
            bootstrap_peer_multiaddr.map(|multiaddr| vec![multiaddr]),
            ChainId::Mainnet,
            node_version,
            DEFAULT_PING_INTERVAL,
            DEFAULT_PING_TIMEOUT,
        )
    });
    swarm.listen_on(Protocol::Memory(0).into()).unwrap();
    swarm
}

/// Poll the swarm to discover its listen address, then set it as an external address.
/// Only safe to call on swarms without bootstrap peers (no events will be lost).
async fn get_listen_address(swarm: &mut Swarm<MixedBehaviour>) -> Multiaddr {
    let address = swarm
        .wait(|event| match event {
            SwarmEvent::NewListenAddr { address, .. } => Some(address),
            _ => None,
        })
        .await;
    swarm.add_external_address(address.clone());
    address
}

fn create_network_manager(
    swarm: Swarm<MixedBehaviour>,
) -> GenericNetworkManager<Swarm<MixedBehaviour>> {
    let advertised_multiaddr = None;
    let metrics = None;
    GenericNetworkManager::generic_new(
        swarm,
        advertised_multiaddr,
        metrics,
        MESSAGE_METADATA_BUFFER_SIZE,
        MESSAGE_METADATA_BUFFER_SIZE,
        HashSet::new(),
    )
}

fn create_reporting_network_manager(
    swarm: ConnectionReportingSwarm,
) -> GenericNetworkManager<ConnectionReportingSwarm> {
    let advertised_multiaddr = None;
    let metrics = None;
    GenericNetworkManager::generic_new(
        swarm,
        advertised_multiaddr,
        metrics,
        MESSAGE_METADATA_BUFFER_SIZE,
        MESSAGE_METADATA_BUFFER_SIZE,
        HashSet::new(),
    )
}
