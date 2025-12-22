//! Propeller network behaviour (libp2p adapter).
//!
//! This file implements the `libp2p::swarm::NetworkBehaviour` and forwards all protocol logic to
//! the core in `core.rs`, which implements `futures::Stream` and is polled here.

use std::collections::VecDeque;
use std::task::{Context, Poll};

use libp2p::core::Endpoint;
use libp2p::identity::{Keypair, PeerId};
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

use crate::channel_utils::try_send_critical;
use crate::config::Config;
use crate::core::{Core, CoreCommand, CoreOutput};
use crate::deadline_wrapper::spawn_monitored;
use crate::handler::{Handler, HandlerIn, HandlerOut};
use crate::metrics::PropellerMetrics;
use crate::types::{Channel, Event, MessageRoot, PeerSetError, ShardPublishError};
// use crate::unit::PropellerUnit;

/// Determines the authenticity requirements for messages.
#[derive(Clone)]
pub enum MessageAuthenticity {
    /// Message signing is enabled. The author will be the owner of the key.
    Signed(Keypair),
    /// Message signing is disabled. The specified peer id will be used as the author.
    Author(PeerId),
}

/// The Propeller network behaviour (thin adapter around `core.rs`).
pub struct Behaviour {
    /// Configuration for this behaviour.
    config: Config,

    /// Events to be returned to the swarm.
    events: VecDeque<ToSwarm<Event, HandlerIn>>,

    /// Channel to send commands to Core task.
    core_commands_tx: mpsc::Sender<CoreCommand>,

    /// Channel to receive outputs from Core task.
    core_outputs_rx: mpsc::Receiver<CoreOutput>,
}

impl Behaviour {
    /// Create a new Propeller behaviour.
    pub fn new(message_authenticity: MessageAuthenticity, config: Config) -> Self {
        Self::new_with_metrics(message_authenticity, config, None)
    }

    /// Create a new Propeller behaviour with optional metrics.
    pub fn new_with_metrics(
        message_authenticity: MessageAuthenticity,
        config: Config,
        metrics: Option<PropellerMetrics>,
    ) -> Self {
        // Create bounded channels with backpressure
        let (commands_tx, commands_rx) = mpsc::channel(config.channel_capacity());
        let (outputs_tx, outputs_rx) = mpsc::channel(config.channel_capacity());

        let core = Core::new(message_authenticity, config.clone(), metrics);

        // Spawn the core task
        spawn_monitored("core_task", async move {
            core.run(commands_rx, outputs_tx).await;
        });

        Self {
            config,
            events: VecDeque::new(),
            core_commands_tx: commands_tx,
            core_outputs_rx: outputs_rx,
        }
    }

    fn update_events_queue_metric(&mut self) {
        let len = self.events.len();
        try_send_critical(
            &self.core_commands_tx,
            CoreCommand::UpdateEventsQueueLen { len },
            "Behaviour->Core",
        );
    }

    fn push_swarm_event(&mut self, event: ToSwarm<Event, HandlerIn>) {
        self.events.push_back(event);
        self.update_events_queue_metric();
    }

    fn pop_swarm_event(&mut self) -> Option<ToSwarm<Event, HandlerIn>> {
        let ev = self.events.pop_front();
        if ev.is_some() {
            self.update_events_queue_metric();
        }
        ev
    }

    /// Register a channel with multiple peers and their weights for tree topology calculation.
    pub async fn register_channel_peers(
        &mut self,
        channel: Channel,
        peers: impl IntoIterator<Item = (PeerId, u64)>,
    ) -> Result<(), PeerSetError> {
        self.register_channel_peers_and_optional_keys(
            channel,
            peers.into_iter().map(|(peer_id, weight)| (peer_id, weight, None)),
        )
        .await
    }

    /// Register a channel with peers and explicit public keys for signature verification.
    pub async fn register_channel_peers_and_keys(
        &mut self,
        channel: Channel,
        peers: impl IntoIterator<Item = (PeerId, u64, libp2p::identity::PublicKey)>,
    ) -> Result<(), PeerSetError> {
        self.register_channel_peers_and_optional_keys(
            channel,
            peers
                .into_iter()
                .map(|(peer_id, weight, public_key)| (peer_id, weight, Some(public_key))),
        )
        .await
    }

    /// Register a channel with peers and optional public keys.
    pub async fn register_channel_peers_and_optional_keys(
        &mut self,
        channel: Channel,
        peers: impl IntoIterator<Item = (PeerId, u64, Option<libp2p::identity::PublicKey>)>,
    ) -> Result<(), PeerSetError> {
        let (tx, rx) = oneshot::channel();
        try_send_critical(
            &self.core_commands_tx,
            CoreCommand::RegisterChannelPeers {
                channel,
                peers: peers.into_iter().collect(),
                response: tx,
            },
            "Behaviour->Core",
        );

        rx.await.expect("Core task dropped response channel - this is a critical bug")
    }

    /// Get the number of peers this node knows about on a specific channel (including itself).
    pub async fn peer_count(&mut self, channel: Channel) -> Option<usize> {
        let (tx, rx) = oneshot::channel();
        try_send_critical(
            &self.core_commands_tx,
            CoreCommand::PeerCount { channel, response: tx },
            "Behaviour->Core",
        );

        rx.await.expect("Core task dropped response channel - this is a critical bug")
    }

    /// Get all registered channels.
    pub async fn registered_channels(&mut self) -> Vec<Channel> {
        let (tx, rx) = oneshot::channel();
        try_send_critical(
            &self.core_commands_tx,
            CoreCommand::RegisteredChannels { response: tx },
            "Behaviour->Core",
        );

        rx.await.expect("Core task dropped response channel - this is a critical bug")
    }

    // pub fn prepare_units(
    //     channel: Channel,
    //     publisher: PeerId,
    //     keypair: Option<Keypair>,
    //     message: Vec<u8>,
    //     pad: bool,
    //     num_data_shards: usize,
    //     num_coding_shards: usize,
    // ) -> Result<Vec<PropellerUnit>, ShardPublishError> {
    //     Core::prepare_units(
    //         channel,
    //         publisher,
    //         keypair,
    //         message,
    //         pad,
    //         num_data_shards,
    //         num_coding_shards,
    //     )
    // }

    pub async fn broadcast(
        &mut self,
        channel: Channel,
        message: Vec<u8>,
    ) -> Result<MessageRoot, ShardPublishError> {
        let (tx, rx) = oneshot::channel();
        try_send_critical(
            &self.core_commands_tx,
            CoreCommand::Broadcast { channel, message, response: tx },
            "Behaviour->Core",
        );

        rx.await.expect("Core task closed unexpectedly - this is a critical bug")
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
        Ok(Handler::new(
            self.config.stream_protocol().clone(),
            self.config.max_wire_message_size(),
            self.config.substream_timeout(),
        ))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &libp2p::core::Multiaddr,
        _role_override: Endpoint,
        _port_use: libp2p::core::transport::PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(
            self.config.stream_protocol().clone(),
            self.config.max_wire_message_size(),
            self.config.substream_timeout(),
        ))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. }) => {
                try_send_critical(
                    &self.core_commands_tx,
                    CoreCommand::HandleConnected { peer_id },
                    "Behaviour->Core",
                );
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) => {
                if remaining_established == 0 {
                    try_send_critical(
                        &self.core_commands_tx,
                        CoreCommand::HandleDisconnected { peer_id },
                        "Behaviour->Core",
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
        match event {
            HandlerOut::Unit(unit) => {
                try_send_critical(
                    &self.core_commands_tx,
                    CoreCommand::HandleUnit { sender: peer_id, unit },
                    "Behaviour->Core",
                );
            }
            HandlerOut::SendError(error) => {
                try_send_critical(
                    &self.core_commands_tx,
                    CoreCommand::HandleSendError { peer_id, error },
                    "Behaviour->Core",
                );
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        // Poll for outputs from the core task
        // This automatically registers cx.waker() with the channel!
        loop {
            match self.core_outputs_rx.poll_recv(cx) {
                Poll::Ready(Some(output)) => match output {
                    CoreOutput::GenerateEvent(event) => {
                        self.push_swarm_event(ToSwarm::GenerateEvent(event));
                    }
                    CoreOutput::NotifyHandler { peer_id, event } => {
                        self.push_swarm_event(ToSwarm::NotifyHandler {
                            peer_id,
                            handler: NotifyHandler::Any,
                            event,
                        });
                    }
                },
                Poll::Ready(None) => {
                    // Core task ended
                    break;
                }
                Poll::Pending => break,
            }
        }

        // Return any pending events
        if let Some(event) = self.pop_swarm_event() {
            return Poll::Ready(event);
        }

        Poll::Pending
    }
}
