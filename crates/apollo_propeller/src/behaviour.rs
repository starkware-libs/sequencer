//! Propeller network behaviour (libp2p adapter).
//!
//! This module implements the libp2p `NetworkBehaviour` trait for the Propeller protocol,
//!
//! # Overview
//!
//! The Propeller protocol uses erasure coding and tree-based routing to broadcast messages
//! efficiently across a network of peers. The `Behaviour` struct serves as the main interface
//! between libp2p's networking stack and the Propeller protocol engine.

use std::task::{Context, Poll};

use libp2p::core::Endpoint;
use libp2p::identity::{Keypair, PeerId, PublicKey};
use libp2p::swarm::behaviour::{ConnectionClosed, ConnectionEstablished, FromSwarm};
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionId,
    NetworkBehaviour,
    NotifyHandler,
    THandler,
    THandlerInEvent,
    THandlerOutEvent,
    ToSwarm,
};
use starknet_api::staking::StakingWeight;
use tokio::sync::{mpsc, oneshot};

use crate::config::Config;
use crate::engine::{Engine, EngineCommand, EngineOutput};
use crate::handler::Handler;
use crate::metrics::PropellerMetrics;
use crate::types::{CommitteeId, CommitteeSetupError, Event, ShardPublishError};

/// The Propeller network behaviour.
///
/// This struct implements the libp2p `NetworkBehaviour` trait and serves as the main entry
/// point for interacting with the Propeller protocol. It manages committee registrations,
/// message broadcasting, and coordination with the underlying protocol engine.
pub struct Behaviour {
    config: Config,
    engine_commands_tx: mpsc::UnboundedSender<EngineCommand>,
    engine_outputs_rx: mpsc::UnboundedReceiver<EngineOutput>,
}

impl Behaviour {
    pub fn new(keypair: Keypair, config: Config) -> Self {
        Self::new_with_metrics(keypair, config, None)
    }

    pub fn new_with_metrics(
        keypair: Keypair,
        config: Config,
        metrics: Option<PropellerMetrics>,
    ) -> Self {
        let (engine_commands_tx, engine_commands_rx) = mpsc::unbounded_channel();
        let (engine_outputs_tx, engine_outputs_rx) = mpsc::unbounded_channel();
        let engine =
            Engine::new(keypair, config.clone(), engine_commands_rx, engine_outputs_tx, metrics);
        tokio::spawn(async move {
            engine.run().await;
        });
        Self { config, engine_commands_tx, engine_outputs_rx }
    }

    // TODO(AndrewL): change register_committee to return the committee id.
    // TODO(AndrewL): update the propeller API to speak in terms of StakerID instead of PeerId
    // and remove all public key related code (register_committee_peers_and_optional_keys,
    // PublicKey import, etc.).

    /// Register peers for a committee where public keys are embedded in peer IDs (Ed25519,
    /// Secp256k1).
    ///
    /// Use when all peers have public keys embedded in their peer IDs. For peers with RSA keys,
    /// use `register_committee_peers_and_optional_keys` instead.
    pub fn register_committee_peers(
        &self,
        committee_id: CommitteeId,
        peers: Vec<(PeerId, StakingWeight)>,
    ) -> oneshot::Receiver<Result<(), CommitteeSetupError>> {
        self.register_committee_peers_and_optional_keys(
            committee_id,
            peers.into_iter().map(|(peer_id, weight)| (peer_id, weight, None)).collect(),
        )
    }

    /// Register peers for a committee with optional explicit public keys.
    ///
    /// Use when some peers require explicit public keys (e.g., RSA peer IDs where keys cannot be
    /// derived from peer IDs). Provide `None` for peers with embedded keys (Ed25519, Secp256k1).
    pub fn register_committee_peers_and_optional_keys(
        &self,
        committee_id: CommitteeId,
        peers: Vec<(PeerId, StakingWeight, Option<PublicKey>)>,
    ) -> oneshot::Receiver<Result<(), CommitteeSetupError>> {
        let (response_tx, response_rx) = oneshot::channel();
        let command =
            EngineCommand::RegisterCommitteePeers { committee_id, peers, response: response_tx };
        self.engine_commands_tx.send(command).expect("Engine task has exited");
        response_rx
    }

    /// Unregister a committee and clean up all associated state.
    // TODO(AndrewL): Reconsider whether unregister_committee should exist here or if committee
    // lifecycle should be managed by an LRU cache in the network manager instead.
    pub fn unregister_committee(&self, committee_id: CommitteeId) -> oneshot::Receiver<bool> {
        let (response_tx, response_rx) = oneshot::channel();
        let command = EngineCommand::UnregisterCommittee { committee_id, response: response_tx };
        self.engine_commands_tx.send(command).expect("Engine task has exited");
        response_rx
    }

    /// Broadcast a message to all peers in a committee using erasure coding.
    ///
    /// Returns a receiver that will receive the result of the broadcast.
    pub fn broadcast(
        &self,
        committee_id: CommitteeId,
        message: Vec<u8>,
    ) -> oneshot::Receiver<Result<(), ShardPublishError>> {
        let (response_tx, response_rx) = oneshot::channel();
        let command = EngineCommand::Broadcast { committee_id, message, response_tx };
        self.engine_commands_tx.send(command).expect("Engine task has exited");
        response_rx
    }

    /// Creates a handler for a new connection, wiring a bounded channel between the handler
    /// and the engine for inbound unit delivery.
    fn create_handler(&self, peer_id: PeerId) -> Handler {
        let (sender, receiver) =
            futures::channel::mpsc::channel(self.config.inbound_channel_capacity);
        let command = EngineCommand::RegisterHandler { peer_id, receiver };
        self.engine_commands_tx.send(command).expect("Engine task has exited");
        Handler::new(&self.config, sender)
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        _local_addr: &libp2p::core::Multiaddr,
        _remote_addr: &libp2p::core::Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self.create_handler(peer))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        _addr: &libp2p::core::Multiaddr,
        _role_override: Endpoint,
        _port_use: libp2p::core::transport::PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self.create_handler(peer))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                other_established,
                ..
            }) => {
                if other_established == 0 {
                    let command = EngineCommand::HandleConnected { peer_id };
                    self.engine_commands_tx.send(command).expect("Engine task has exited");
                }
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) => {
                if remaining_established == 0 {
                    let command = EngineCommand::HandleDisconnected { peer_id };
                    self.engine_commands_tx.send(command).expect("Engine task has exited");
                }
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        let command = EngineCommand::HandleHandlerOutput { peer_id, output: event };
        self.engine_commands_tx.send(command).expect("Engine task has exited");
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        match self.engine_outputs_rx.poll_recv(cx) {
            Poll::Ready(Some(output)) => Poll::Ready(match output {
                EngineOutput::GenerateEvent(event) => ToSwarm::GenerateEvent(event),
                EngineOutput::NotifyHandler { peer_id, event } => {
                    ToSwarm::NotifyHandler { peer_id, handler: NotifyHandler::Any, event }
                }
            }),
            Poll::Ready(None) => {
                unreachable!("Engine task closed unexpectedly - this is a critical bug");
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
