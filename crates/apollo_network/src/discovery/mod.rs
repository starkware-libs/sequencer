#[cfg(test)]
mod discovery_test;
#[cfg(test)]
mod flow_test;
pub mod identify_impl;
pub mod kad_impl;

use std::collections::BTreeMap;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
};
use apollo_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use futures::future::BoxFuture;
use futures::FutureExt;
use libp2p::core::Endpoint;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{
    dummy,
    AddressChange,
    ConnectionClosed,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    DialFailure,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use tokio_retry::strategy::ExponentialBackoff;

use crate::mixed_behaviour;
use crate::mixed_behaviour::BridgedBehaviour;

pub struct Behaviour {
    config: DiscoveryConfig,
    bootstrap_peer_address: Multiaddr,
    bootstrap_peer_id: PeerId,
    is_dialing_to_bootstrap_peer: bool,
    is_connected_to_bootstrap_peer: bool,
    is_bootstrap_in_kad_routing_table: bool,
    bootstrap_dial_retry_strategy: ExponentialBackoff,
    time_for_next_bootstrap_dial: Instant,
    time_for_next_kad_query: Instant,
    // Storing the future that contains the code logic for polling inside the behaviour because the
    // code contains a sleep and if a sleep is reconstructed every poll then it will sleep for much
    // more than the input time.
    poll_future: Option<BoxFuture<'static, PollFutureOutput>>,
}

#[derive(Debug)]
pub enum ToOtherBehaviourEvent {
    RequestKadQuery(PeerId),
    FoundListenAddresses { peer_id: PeerId, listen_addresses: Vec<Multiaddr> },
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = ToOtherBehaviourEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::DialFailure(DialFailure { peer_id: Some(peer_id), .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                self.is_dialing_to_bootstrap_peer = false;
                // For the case that the reason for failure is consistent (e.g the bootstrap peer
                // is down), we sleep before redialing
                self.time_for_next_bootstrap_dial = Instant::now()
                    + self
                        .bootstrap_dial_retry_strategy
                        .next()
                        .expect("Dial sleep strategy ended even though it's an infinite iterator.");
            }
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                self.is_connected_to_bootstrap_peer = true;
                self.is_dialing_to_bootstrap_peer = false;
                self.bootstrap_dial_retry_strategy =
                    self.config.bootstrap_dial_retry_config.strategy();
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) if peer_id == self.bootstrap_peer_id && remaining_established == 0 => {
                self.is_connected_to_bootstrap_peer = false;
                self.is_dialing_to_bootstrap_peer = false;
                self.is_bootstrap_in_kad_routing_table = false;
            }
            FromSwarm::AddressChange(AddressChange { peer_id, .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                todo!();
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        let is_connected_to_bootstrap_peer = self.is_connected_to_bootstrap_peer;
        let is_bootstrap_in_kad_routing_table = self.is_bootstrap_in_kad_routing_table;
        let bootstrap_peer_id = self.bootstrap_peer_id;
        let bootstrap_peer_address = self.bootstrap_peer_address.clone();
        let time_for_next_bootstrap_dial = self.time_for_next_bootstrap_dial;
        let time_for_next_kad_query = self.time_for_next_kad_query;
        let is_dialing_to_bootstrap_peer = self.is_dialing_to_bootstrap_peer;
        let heartbeat_interval = self.config.heartbeat_interval;
        let mut poll_future = self.poll_future.take().unwrap_or(
            async move {
                if is_connected_to_bootstrap_peer && !is_bootstrap_in_kad_routing_table {
                    return PollFutureOutput {
                        event: ToSwarm::GenerateEvent(
                            ToOtherBehaviourEvent::FoundListenAddresses {
                                peer_id: bootstrap_peer_id,
                                listen_addresses: vec![bootstrap_peer_address],
                            },
                        ),
                        is_bootstrap_in_kad_routing_table: Some(true),
                        is_dialing_to_bootstrap_peer: None,
                        time_for_next_kad_query: None,
                    };
                }

                // TODO(Shahak): If one of the last two conditions is false, register a waker and
                // wake it when we receive an event that we've disconnected from the bootstrap peer.
                // (Right now, when we're disconnected from the bootstrap peer, we'll wait for next
                // kad query even if time_for_next_bootstrap_dial is smaller than
                // time_for_next_kad_query)
                if time_for_next_bootstrap_dial < time_for_next_kad_query
                    // No need to perform a dial if there's an active dial attempt or we're already
                    // connected.
                    && !(is_dialing_to_bootstrap_peer)
                    && !(is_connected_to_bootstrap_peer)
                {
                    tokio::time::sleep_until(time_for_next_bootstrap_dial.into()).await;
                    PollFutureOutput {
                        event: ToSwarm::Dial {
                            opts: DialOpts::peer_id(bootstrap_peer_id)
                                .addresses(vec![bootstrap_peer_address])
                                // The peer manager might also be dialing to the bootstrap node.
                                .condition(PeerCondition::DisconnectedAndNotDialing)
                                .build(),
                        },
                        is_dialing_to_bootstrap_peer: Some(true),
                        is_bootstrap_in_kad_routing_table: None,
                        time_for_next_kad_query: None,
                    }
                } else {
                    tokio::time::sleep_until(time_for_next_kad_query.into()).await;
                    PollFutureOutput {
                        event: ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(
                            libp2p::identity::PeerId::random(),
                        )),
                        time_for_next_kad_query: Some(Instant::now() + heartbeat_interval),
                        is_bootstrap_in_kad_routing_table: None,
                        is_dialing_to_bootstrap_peer: None,
                    }
                }
            }
            .boxed(),
        );
        let output = poll_future.poll_unpin(cx);
        match output {
            Poll::Ready(PollFutureOutput {
                event,
                is_bootstrap_in_kad_routing_table,
                is_dialing_to_bootstrap_peer,
                time_for_next_kad_query,
            }) => {
                self.poll_future = None;
                self.is_bootstrap_in_kad_routing_table = is_bootstrap_in_kad_routing_table
                    .unwrap_or(self.is_bootstrap_in_kad_routing_table);
                self.is_dialing_to_bootstrap_peer =
                    is_dialing_to_bootstrap_peer.unwrap_or(self.is_dialing_to_bootstrap_peer);
                self.time_for_next_kad_query =
                    time_for_next_kad_query.unwrap_or(self.time_for_next_kad_query);
                Poll::Ready(event)
            }
            Poll::Pending => {
                self.poll_future = Some(poll_future);
                Poll::Pending
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryConfig {
    pub bootstrap_dial_retry_config: RetryConfig,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub heartbeat_interval: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            bootstrap_dial_retry_config: RetryConfig::default(),
            heartbeat_interval: Duration::from_millis(100),
        }
    }
}

impl SerializeConfig for DiscoveryConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from([ser_param(
            "heartbeat_interval",
            &self.heartbeat_interval.as_millis(),
            "The interval between each discovery (Kademlia) query in milliseconds.",
            ParamPrivacyInput::Public,
        )]);
        dump.append(&mut append_sub_config_name(
            self.bootstrap_dial_retry_config.dump(),
            "bootstrap_dial_retry_config",
        ));
        dump
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetryConfig {
    pub base_delay_millis: u64,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub max_delay_seconds: Duration,
    pub factor: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self { base_delay_millis: 2, max_delay_seconds: Duration::from_secs(5), factor: 5 }
    }
}

impl SerializeConfig for RetryConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "base_delay_millis",
                &self.base_delay_millis,
                "The base delay in milliseconds for the exponential backoff strategy.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_delay_seconds",
                &self.max_delay_seconds.as_secs(),
                "The maximum delay in seconds for the exponential backoff strategy.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "factor",
                &self.factor,
                "The factor for the exponential backoff strategy.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl RetryConfig {
    fn strategy(&self) -> ExponentialBackoff {
        ExponentialBackoff::from_millis(self.base_delay_millis)
            .max_delay(self.max_delay_seconds)
            .factor(self.factor)
    }
}

impl Behaviour {
    // TODO(shahak): Add support to discovery from multiple bootstrap nodes.
    // TODO(shahak): Add support to multiple addresses for bootstrap node.
    pub fn new(
        config: DiscoveryConfig,
        bootstrap_peer_id: PeerId,
        bootstrap_peer_address: Multiaddr,
    ) -> Self {
        let bootstrap_dial_retry_strategy = config.bootstrap_dial_retry_config.strategy();
        Self {
            config,
            bootstrap_peer_id,
            bootstrap_peer_address,
            is_dialing_to_bootstrap_peer: false,
            is_connected_to_bootstrap_peer: false,
            is_bootstrap_in_kad_routing_table: false,
            bootstrap_dial_retry_strategy,
            time_for_next_bootstrap_dial: Instant::now(),
            time_for_next_kad_query: Instant::now(),
            poll_future: None,
        }
    }

    #[cfg(test)]
    pub fn bootstrap_peer_id(&self) -> PeerId {
        self.bootstrap_peer_id
    }

    #[cfg(test)]
    pub fn bootstrap_peer_address(&self) -> &Multiaddr {
        &self.bootstrap_peer_address
    }
}

impl From<ToOtherBehaviourEvent> for mixed_behaviour::Event {
    fn from(event: ToOtherBehaviourEvent) -> Self {
        mixed_behaviour::Event::ToOtherBehaviourEvent(
            mixed_behaviour::ToOtherBehaviourEvent::Discovery(event),
        )
    }
}

impl BridgedBehaviour for Behaviour {
    fn on_other_behaviour_event(&mut self, _event: &mixed_behaviour::ToOtherBehaviourEvent) {}
}

/// The output of the future that is polled inside poll. This contains the event to be emitted and
/// an Option for each field of [`Behaviour`] that can be changed by polling. If the Option is
/// Some, then that field needs to be changed to the value inside it.
struct PollFutureOutput {
    pub event: ToSwarm<
        ToOtherBehaviourEvent,
        <dummy::ConnectionHandler as ConnectionHandler>::FromBehaviour,
    >,
    pub is_bootstrap_in_kad_routing_table: Option<bool>,
    pub is_dialing_to_bootstrap_peer: Option<bool>,
    pub time_for_next_kad_query: Option<Instant>,
    // No need to set time_for_next_bootstrap_dial since by default we don't sleep before dialing
    // to bootstrap. We set time_for_next_bootstrap_dial only upon dial failure, and dial
    // failure isn't handled inside poll.
}
