// TODO(shahak): Erase main_behaviour and make this a separate module.

use std::convert::Infallible;
use std::time::Duration;

use apollo_propeller::metrics::PropellerMetrics;
use apollo_propeller::{self as propeller};
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
use crate::network_manager::metrics::EventMetrics;
use crate::peer_manager::PeerManagerConfig;
use crate::{discovery, gossipsub_impl, peer_manager, propeller_impl, sqmr};

// TODO(Shahak): consider reducing the pulicity of all behaviour to pub(crate)
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct MixedBehaviour {
    pub limits: connection_limits::Behaviour,
    pub peer_manager: peer_manager::PeerManager,
    pub discovery: Toggle<discovery::Behaviour>,
    pub identify: identify::Behaviour,
    // TODO(shahak): Consider using a different store.
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub sqmr: sqmr::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
    pub propeller: propeller::Behaviour,
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
    Propeller(propeller_impl::ExternalEvent),
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
        propeller_metrics: Option<PropellerMetrics>,
        keypair: Keypair,
        // TODO(AndrewL): consider making this non optional
        bootstrap_peers_multiaddrs: Option<Vec<Multiaddr>>,
        chain_id: ChainId,
        node_version: Option<String>,
    ) -> Self {
        let public_key = keypair.public();
        let local_peer_id = PeerId::from_public_key(&public_key);
        let protocol_name =
            StreamProtocol::try_from_owned(format!("/starknet/kad/{chain_id}/1.0.0"))
                .expect("Failed to create StreamProtocol from a string that starts with /");
        let kademlia_config = kad::Config::new(protocol_name);
        let connection_limits = ConnectionLimits::default(); // .with_max_established_per_peer(Some(1));

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .max_transmit_size(1 << 34)
            .flood_publish(false)
            .heartbeat_interval(Duration::from_millis(700))
            // .validation_mode(ValidationMode::None)
            .message_id_fn(|message| {
                let mut source_string = message.source.as_ref().map(|id| id.to_bytes()).unwrap_or_default();
                source_string
                    .extend_from_slice(&message.sequence_number.unwrap_or_default().to_be_bytes());
                source_string.extend_from_slice(&message.data[0..16]);
                gossipsub::MessageId::from(source_string)
            })
            
            // .max_messages_per_rpc(Some(1))
            .history_length(5)

            // .connection_handler_queue_len(50_000) 

            .mesh_n(10)          // Target mesh peers
            .mesh_n_low(10)       // Minimum before adding (default: 5)
            .mesh_n_high(10)     // Maximum before pruning (default
            // .publish_queue_duration(Duration::from_millis(500))  // default: 5s
            // .forward_queue_duration(Duration::from_millis(100))  // default: 1s
    

            // .heartbeat_interval(Duration::from_millis(200))
            // Set gossip_lazy to 0 - minimum number of peers to gossip to
            .gossip_lazy(0)

            // Set gossip_factor to 0.0 - factor for dynamic gossip peer selection
            .gossip_factor(0.0)

            // Set history_gossip to 0 - number of past heartbeats to gossip about
            .history_gossip(0)

            // Optional: Set max_ihave_length to 0 - maximum IHAVE message IDs per message
            .max_ihave_length(0)

            // Optional: Set max_ihave_messages to 0 - maximum IHAVE messages per heartbeat
            .max_ihave_messages(0)

            // Optional: Set gossip_retransmission to 0 - disable IWANT retries
            .gossip_retransimission(0)
            
            // Enable IDONTWANT optimization
            // .idontwant_message_size_threshold(1000)  // Adjust based on your message sizes
            .idontwant_on_publish(true)              // Prevent echo-back on publish
   
            .build()
            .expect("Failed to build gossipsub config");
        Self {
            limits: connection_limits::Behaviour::new(connection_limits),
            peer_manager: peer_manager::PeerManager::new(peer_manager_config),
            discovery: bootstrap_peers_multiaddrs
                .map(|bootstrap_peer_multiaddr| {
                    discovery::Behaviour::new(
                        local_peer_id,
                        discovery_config,
                        bootstrap_peer_multiaddr
                            .iter()
                            .map(|bootstrap_peer_multiaddr| {
                                (
                                    DialOpts::from(bootstrap_peer_multiaddr.clone())
                                        .get_peer_id()
                                        .expect("bootstrap_peer_multiaddr doesn't have a peer id"),
                                    bootstrap_peer_multiaddr.clone(),
                                )
                            })
                            .collect(),
                    )
                })
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
        gossipsub::MessageAuthenticity::Signed(keypair.clone()),
                gossipsub_config,
            )
            .unwrap_or_else(|err_string| {
                panic!(
                    "Failed creating gossipsub behaviour due to the following error: {err_string}"
                )
            }),
            propeller: propeller::Behaviour::new_with_metrics(
                propeller::MessageAuthenticity::Signed(keypair),
                propeller::ConfigBuilder::default().build(),
                propeller_metrics,
            ),
            event_tracker_metrics: event_metrics.map(EventMetricsTracker::new).into(),
        }
    }
}

impl From<Infallible> for Event {
    fn from(infallible: Infallible) -> Self {
        match infallible {}
    }
}
