#[cfg(test)]
mod discovery_test;
#[cfg(test)]
mod flow_test;
pub mod identify_impl;
pub mod kad_impl;

use std::collections::BTreeMap;
use std::task::{ready, Context, Poll};
use std::time::Duration;

use futures::future::{pending, select, BoxFuture, Either};
use futures::{pin_mut, Future, FutureExt};
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
use papyrus_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
};
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use tokio_retry::strategy::ExponentialBackoff;

use crate::mixed_behaviour;
use crate::mixed_behaviour::BridgedBehaviour;

pub struct Behaviour {
    config: DiscoveryConfig,
    bootstrap_peer_address: Multiaddr,
    bootstrap_peer_id: PeerId,
    is_dialing_to_bootstrap_peer: bool,
    // This needs to be boxed to allow polling it from a &mut.
    sleep_future_for_dialing_bootstrap_peer: Option<BoxFuture<'static, ()>>,
    is_connected_to_bootstrap_peer: bool,
    is_bootstrap_in_kad_routing_table: bool,
    bootstrap_dial_retry_strategy: ExponentialBackoff,
    query_sleep_future: Option<BoxFuture<'static, ()>>,
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
                self.sleep_future_for_dialing_bootstrap_peer = Some(
                    tokio::time::sleep(self.bootstrap_dial_retry_strategy.next().expect(
                        "Dial sleep strategy ended even though it's an infinite iterator.",
                    ))
                    .boxed(),
                );
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
        if self.is_connected_to_bootstrap_peer && !self.is_bootstrap_in_kad_routing_table {
            self.is_bootstrap_in_kad_routing_table = true;
            return Poll::Ready(ToSwarm::GenerateEvent(
                ToOtherBehaviourEvent::FoundListenAddresses {
                    peer_id: self.bootstrap_peer_id,
                    listen_addresses: vec![self.bootstrap_peer_address.clone()],
                },
            ));
        }

        // Unpacking self so that we can create 2 futures that use different members of self
        let Self {
            is_dialing_to_bootstrap_peer,
            is_connected_to_bootstrap_peer,
            sleep_future_for_dialing_bootstrap_peer,
            bootstrap_peer_id,
            bootstrap_peer_address,
            query_sleep_future,
            config,
            ..
        } = self;

        let bootstrap_dial_future = async move {
            if !(*is_dialing_to_bootstrap_peer) && !(*is_connected_to_bootstrap_peer) {
                if let Some(sleep_future) = sleep_future_for_dialing_bootstrap_peer {
                    sleep_future.await;
                }
                *is_dialing_to_bootstrap_peer = true;
                *sleep_future_for_dialing_bootstrap_peer = None;
                return ToSwarm::Dial {
                    opts: DialOpts::peer_id(*bootstrap_peer_id)
                    .addresses(vec![bootstrap_peer_address.clone()])
                    // The peer manager might also be dialing to the bootstrap node.
                    .condition(PeerCondition::DisconnectedAndNotDialing)
                    .build(),
                };
            }
            // We're already connected to the bootstrap peer. Nothing to do
            // TODO: register a waker here and wake it when we receive an event that we've
            // disconnected from the bootstrap peer.
            pending().await
        };
        pin_mut!(bootstrap_dial_future);
        let kad_future = async move {
            if let Some(sleep_future) = query_sleep_future {
                sleep_future.await;
            }
            *query_sleep_future = Some(tokio::time::sleep(config.heartbeat_interval).boxed());
            ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(
                libp2p::identity::PeerId::random(),
            ))
        };
        pin_mut!(kad_future);

        // polling both futures together since each of them contains sleep.
        let select_future = select(bootstrap_dial_future, kad_future);
        pin_mut!(select_future);
        let (Either::Left((event, _)) | Either::Right((event, _))) = ready!(select_future.poll(cx));
        Poll::Ready(event)
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
            sleep_future_for_dialing_bootstrap_peer: None,
            is_connected_to_bootstrap_peer: false,
            is_bootstrap_in_kad_routing_table: false,
            bootstrap_dial_retry_strategy,
            query_sleep_future: None,
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
