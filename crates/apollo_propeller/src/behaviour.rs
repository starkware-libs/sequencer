//! Propeller network behaviour (libp2p adapter).

use std::collections::VecDeque;
use std::task::{Context, Poll};

use libp2p::core::Endpoint;
use libp2p::identity::PeerId;
use libp2p::swarm::behaviour::{ConnectionClosed, ConnectionEstablished, FromSwarm};
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionId,
    NetworkBehaviour,
    THandler,
    THandlerInEvent,
    THandlerOutEvent,
    ToSwarm,
};
use tokio::sync::mpsc;

use crate::config::Config;
use crate::core::{Core, CoreOutput};
use crate::handler::{Handler, HandlerIn, HandlerOut};
use crate::types::Event;

/// The Propeller network behaviour.
pub struct Behaviour {
    /// Configuration for this behaviour.
    config: Config,
    /// Events to be returned to the swarm.
    events: VecDeque<ToSwarm<Event, HandlerIn>>,
    /// Channel to receive outputs from Core task.
    core_outputs_rx: mpsc::Receiver<CoreOutput>,
}

impl Behaviour {
    /// Create a new Propeller behaviour.
    pub fn new(local_peer_id: PeerId, config: Config) -> Self {
        let (commands_tx, commands_rx) = mpsc::channel(config.channel_capacity);
        let (outputs_tx, outputs_rx) = mpsc::channel(config.channel_capacity);

        let core = Core::new(local_peer_id, config.clone());

        // Spawn the core task
        tokio::spawn(async move {
            core.run(commands_rx, outputs_tx).await;
        });

        // TODO(AndrewL): Store commands_tx when we need to send commands to core
        drop(commands_tx);

        Self { config, events: VecDeque::new(), core_outputs_rx: outputs_rx }
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
            FromSwarm::ConnectionEstablished(ConnectionEstablished { .. }) => {}
            FromSwarm::ConnectionClosed(ConnectionClosed { .. }) => {}
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            HandlerOut::Unit(_unit) => {
                // TODO(AndrewL): Send CoreCommand to handle unit
            }
            HandlerOut::SendError(_error) => {
                // TODO(AndrewL): Send CoreCommand to handle error
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        // Poll for outputs from core
        if let Poll::Ready(Some(output)) = self.core_outputs_rx.poll_recv(cx) {
            todo!("Handle core output: {:?}", output);
        }

        // Return any pending events
        if let Some(event) = self.events.pop_front() {
            return Poll::Ready(event);
        }

        Poll::Pending
    }
}
