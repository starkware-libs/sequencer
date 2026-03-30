// TODO(shahak): Erase main_behaviour and make this a separate module.

use std::convert::Infallible;
use std::time::Duration;

use libp2p::connection_limits::ConnectionLimits;
use libp2p::identity::Keypair;
use libp2p::kad::store::MemoryStore;
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{connection_limits, gossipsub, identify, kad, Multiaddr, PeerId, StreamProtocol};
use starknet_api::core::ChainId;

use crate::discovery::identify_impl::{IdentifyToOtherBehaviourEvent, IDENTIFY_PROTOCOL_VERSION};
use crate::discovery::kad_impl::KadToOtherBehaviourEvent;
use crate::discovery::DiscoveryConfig;
use crate::event_tracker::EventMetricsTracker;
use crate::metrics::{EventMetrics, LatencyMetrics};
use crate::peer_manager::PeerManagerConfig;
use crate::{
    discovery,
    gossipsub_impl,
    peer_manager,
    peer_whitelist,
    prune_dead_connections,
    sqmr,
};

// TODO(Shahak): consider reducing the pulicity of all behaviour to pub(crate)
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct MixedBehaviour {
    pub limits: connection_limits::Behaviour,
    pub peer_whitelist: peer_whitelist::Behaviour,
    pub peer_manager: peer_manager::PeerManager,
    pub discovery: Toggle<discovery::Behaviour>,
    pub identify: identify::Behaviour,
    // TODO(shahak): Consider using a different store.
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub sqmr: sqmr::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
    pub prune_dead_connections: prune_dead_connections::Behaviour,
    pub event_tracker_metrics: Toggle<EventMetricsTracker>,
}

#[derive(Debug)]
pub enum Event {
    ExternalEvent(ExternalEvent),
    ToOtherBehaviourEvent(ToOtherBehaviourEvent),
}

#[derive(Debug)]
pub enum ExternalEvent {
    Sqmr(sqmr::behaviour::ExternalEvent),
    GossipSub(gossipsub_impl::ExternalEvent),
}

#[derive(Debug)]
pub enum ToOtherBehaviourEvent {
    NoOp,
    Identify(IdentifyToOtherBehaviourEvent),
    Kad(KadToOtherBehaviourEvent),
    Discovery(discovery::ToOtherBehaviourEvent),
    PeerManager(peer_manager::ToOtherBehaviourEvent),
    Sqmr(sqmr::ToOtherBehaviourEvent),
}

pub trait BridgedBehaviour {
    fn on_other_behaviour_event(&mut self, event: &ToOtherBehaviourEvent);
}

impl MixedBehaviour {
    // TODO(Shahak): get config details from network manager config
    /// Panics if bootstrap_peer_multiaddr doesn't have a peer id.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        streamed_bytes_config: sqmr::Config,
        discovery_config: DiscoveryConfig,
        peer_manager_config: PeerManagerConfig,
        event_metrics: Option<EventMetrics>,
        latency_metrics: Option<LatencyMetrics>,
        keypair: Keypair,
        // TODO(AndrewL): consider making this non optional
        bootstrap_peers_multiaddrs: Option<Vec<Multiaddr>>,
        chain_id: ChainId,
        node_version: Option<String>,
        prune_dead_connections_ping_interval: Duration,
        prune_dead_connections_ping_timeout: Duration,
    ) -> Self {
        let public_key = keypair.public();
        let local_peer_id = PeerId::from_public_key(&public_key);
        let protocol_name =
            StreamProtocol::try_from_owned(format!("/starknet/kad/{chain_id}/1.0.0"))
                .expect("Failed to create StreamProtocol from a string that starts with /");
        let kademlia_config = kad::Config::new(protocol_name);
        let connection_limits = ConnectionLimits::default().with_max_established_per_peer(Some(1));

        let bootstrap_peers_with_ids: Option<Vec<(PeerId, Multiaddr)>> = bootstrap_peers_multiaddrs
            .map(|addrs| {
                addrs
                    .iter()
                    .map(|addr| {
                        (
                            DialOpts::from(addr.clone())
                                .get_peer_id()
                                .expect("bootstrap_peer_multiaddr doesn't have a peer id"),
                            addr.clone(),
                        )
                    })
                    .collect()
            });

        Self {
            limits: connection_limits::Behaviour::new(connection_limits),
            peer_whitelist: peer_whitelist::Behaviour::new(),
            peer_manager: peer_manager::PeerManager::new(peer_manager_config),
            discovery: bootstrap_peers_with_ids
                .map(|peers| discovery::Behaviour::new(local_peer_id, discovery_config, peers))
                .into(),
            identify: match node_version {
                Some(version) => identify::Behaviour::new(
                    identify::Config::new(IDENTIFY_PROTOCOL_VERSION.to_string(), public_key)
                        .with_agent_version(version),
                ),
                None => identify::Behaviour::new(identify::Config::new(
                    IDENTIFY_PROTOCOL_VERSION.to_string(),
                    public_key,
                )),
            },
            // TODO(Shahak): change kademlia protocol name
            kademlia: kad::Behaviour::with_config(
                local_peer_id,
                MemoryStore::new(local_peer_id),
                kademlia_config,
            ),
            sqmr: sqmr::Behaviour::new(streamed_bytes_config),
            gossipsub: gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(keypair),
                gossipsub::ConfigBuilder::default()
                    // TODO(shahak): try to reduce this bound.
                    .max_transmit_size(1 << 34)
                    .connection_handler_queue_len(20_000)
                    .build()
                    .expect("Failed to build gossipsub config"),
            )
            .unwrap_or_else(|err_string| {
                panic!(
                    "Failed creating gossipsub behaviour due to the following error: {err_string}"
                )
            }),
            prune_dead_connections: prune_dead_connections::Behaviour::new(
                prune_dead_connections_ping_interval,
                prune_dead_connections_ping_timeout,
                latency_metrics,
            ),
            event_tracker_metrics: event_metrics.map(EventMetricsTracker::new).into(),
        }
    }

    /// Routes an internal event to all sub-behaviours that implement `BridgedBehaviour`.
    pub fn route_to_other_behaviour_event(&mut self, event: ToOtherBehaviourEvent) {
        if let ToOtherBehaviourEvent::NoOp = event {
            return;
        }
        self.identify.on_other_behaviour_event(&event);
        self.kademlia.on_other_behaviour_event(&event);
        if let Some(discovery) = self.discovery.as_mut() {
            discovery.on_other_behaviour_event(&event);
        }
        self.sqmr.on_other_behaviour_event(&event);
        self.peer_manager.on_other_behaviour_event(&event);
        self.gossipsub.on_other_behaviour_event(&event);
    }
}

impl From<Infallible> for Event {
    fn from(infallible: Infallible) -> Self {
        match infallible {}
    }
}
