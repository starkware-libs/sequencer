// TODO(shahak): Erase main_behaviour and make this a separate module.

use std::convert::Infallible;

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
use crate::peer_manager::PeerManagerConfig;
use crate::{discovery, gossipsub_impl, peer_manager, sqmr};

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
    pub fn new(
        keypair: Keypair,
        // TODO(AndrewL): consider making this non optional
        bootstrap_peers_multiaddrs: Option<Vec<Multiaddr>>,
        streamed_bytes_config: sqmr::Config,
        chain_id: ChainId,
        node_version: Option<String>,
        discovery_config: DiscoveryConfig,
        peer_manager_config: PeerManagerConfig,
    ) -> Self {
        let public_key = keypair.public();
        let local_peer_id = PeerId::from_public_key(&public_key);
        let protocol_name =
            StreamProtocol::try_from_owned(format!("/starknet/kad/{chain_id}/1.0.0"))
                .expect("Failed to create StreamProtocol from a string that starts with /");
        let kademlia_config = kad::Config::new(protocol_name);
        let connection_limits = ConnectionLimits::default().with_max_established_per_peer(Some(1));

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
                gossipsub::MessageAuthenticity::Signed(keypair),
                gossipsub::ConfigBuilder::default()
                    // TODO(shahak): try to reduce this bound.
                    .max_transmit_size(1 << 34)
                    .build()
                    .expect("Failed to build gossipsub config"),
            )
            .unwrap_or_else(|err_string| {
                panic!(
                    "Failed creating gossipsub behaviour due to the following error: {err_string}"
                )
            }),
        }
    }
}

impl From<Infallible> for Event {
    fn from(infallible: Infallible) -> Self {
        match infallible {}
    }
}
