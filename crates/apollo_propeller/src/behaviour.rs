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
use tokio::sync::{mpsc, oneshot};

use crate::config::Config;
use crate::engine::{Engine, EngineCommand, EngineOutput};
use crate::handler::Handler;
use crate::types::{Channel, Event, MessageRoot, PeerSetError, ShardPublishError};

fn send_unbounded(
    engine_commands_tx: &mpsc::UnboundedSender<EngineCommand>,
    command: EngineCommand,
) {
    engine_commands_tx.send(command).expect("Engine task has exited");
}

/// The Propeller network behaviour.
///
/// This struct implements the libp2p `NetworkBehaviour` trait and serves as the main entry
/// point for interacting with the Propeller protocol. It manages channel registrations,
/// message broadcasting, and coordination with the underlying protocol engine.
pub struct Behaviour {
    config: Config,
    engine_commands_tx: mpsc::UnboundedSender<EngineCommand>,
    engine_outputs_rx: mpsc::UnboundedReceiver<EngineOutput>,
}

impl Behaviour {
    pub fn new(keypair: Keypair, config: Config) -> Self {
        let (commands_tx, commands_rx) = mpsc::unbounded_channel();
        let (outputs_tx, outputs_rx) = mpsc::unbounded_channel();
        let engine = Engine::new(keypair, config.clone(), None, outputs_tx);
        tokio::spawn(async move {
            engine.run(commands_rx).await;
        });
        Self { config, engine_commands_tx: commands_tx, engine_outputs_rx: outputs_rx }
    }

    // TODO(AndrewL): change register channel to return the channel id.

    /// Register peers for a channel where public keys are embedded in peer IDs (Ed25519,
    /// Secp256k1).
    ///
    /// Use when all peers have public keys embedded in their peer IDs. For peers with RSA keys,
    /// use `register_channel_peers_and_optional_keys` instead.
    pub async fn register_channel_peers(
        &mut self,
        channel: Channel,
        peers: Vec<(PeerId, u64)>,
    ) -> Result<(), PeerSetError> {
        self.register_channel_peers_and_optional_keys(
            channel,
            peers.into_iter().map(|(peer_id, weight)| (peer_id, weight, None)).collect(),
        )
        .await
    }

    /// Register peers for a channel with optional explicit public keys.
    ///
    /// Use when some peers require explicit public keys (e.g., RSA peer IDs where keys cannot be
    /// derived from peer IDs). Provide `None` for peers with embedded keys (Ed25519, Secp256k1).
    pub async fn register_channel_peers_and_optional_keys(
        &mut self,
        channel: Channel,
        peers: Vec<(PeerId, u64, Option<PublicKey>)>,
    ) -> Result<(), PeerSetError> {
        let (tx, rx) = oneshot::channel();
        send_unbounded(
            &self.engine_commands_tx,
            EngineCommand::RegisterChannelPeers { channel, peers, response: tx },
        );
        rx.await.expect("Engine task has exited")
    }

    /// Unregister a channel and clean up all associated state.
    pub async fn unregister_channel(&mut self, channel: Channel) -> Result<(), ()> {
        let (tx, rx) = oneshot::channel();
        send_unbounded(
            &self.engine_commands_tx,
            EngineCommand::UnregisterChannel { channel, response: tx },
        );
        rx.await.expect("Engine task has exited")
    }

    /// Broadcast a message to all peers in a channel using erasure coding.
    ///
    /// Returns the Merkle root hash of the message, which serves as a unique identifier.
    pub async fn broadcast(
        &mut self,
        channel: Channel,
        message: Vec<u8>,
    ) -> Result<MessageRoot, ShardPublishError> {
        let (tx, rx) = oneshot::channel();
        send_unbounded(
            &self.engine_commands_tx,
            EngineCommand::Broadcast { channel, message, response: tx },
        );
        rx.await.expect("Engine task has exited")
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &libp2p::core::Multiaddr,
        _remote_addr: &libp2p::core::Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(self.config.stream_protocol.clone(), self.config.max_wire_message_size))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &libp2p::core::Multiaddr,
        _role_override: Endpoint,
        _port_use: libp2p::core::transport::PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(self.config.stream_protocol.clone(), self.config.max_wire_message_size))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                other_established,
                ..
            }) => {
                if other_established == 0 {
                    send_unbounded(
                        &self.engine_commands_tx,
                        EngineCommand::HandleConnected { peer_id },
                    );
                }
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) => {
                if remaining_established == 0 {
                    send_unbounded(
                        &self.engine_commands_tx,
                        EngineCommand::HandleDisconnected { peer_id },
                    );
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
        send_unbounded(
            &self.engine_commands_tx,
            EngineCommand::HandleHandlerOutput { peer_id, output: event },
        );
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
