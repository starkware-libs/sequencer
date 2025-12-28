//! Connection handler for the Propeller protocol.

use std::collections::VecDeque;
use std::task::{Context, Poll};

use libp2p::swarm::handler::{ConnectionEvent, ConnectionHandler, ConnectionHandlerEvent};
use libp2p::swarm::{StreamProtocol, SubstreamProtocol};

use crate::protocol::PropellerProtocol;
use crate::PropellerUnit;

/// Events that the handler can send to the behaviour.
#[derive(Debug)]
pub enum HandlerOut {
    /// A unit was received from the remote peer.
    Unit(PropellerUnit),
    /// An error occurred while sending a message.
    SendError(String),
}

/// Events that the behaviour can send to the handler.
#[derive(Debug, Clone)]
pub enum HandlerIn {
    /// Send a unit to the remote peer.
    SendUnit(PropellerUnit),
}

/// Protocol Handler that manages substreams with a peer.
pub struct Handler {
    /// Upgrade configuration for the propeller protocol.
    listen_protocol: PropellerProtocol,
    /// Queue of messages to send.
    send_queue: VecDeque<PropellerUnit>,
}

impl Handler {
    /// Builds a new [`Handler`].
    pub fn new(stream_protocol: StreamProtocol, max_wire_message_size: usize) -> Self {
        let protocol = PropellerProtocol::new(stream_protocol, max_wire_message_size);
        Handler { listen_protocol: protocol, send_queue: VecDeque::new() }
    }
}

impl ConnectionHandler for Handler {
    type FromBehaviour = HandlerIn;
    type ToBehaviour = HandlerOut;
    type InboundOpenInfo = ();
    type InboundProtocol = PropellerProtocol;
    type OutboundOpenInfo = ();
    type OutboundProtocol = PropellerProtocol;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        SubstreamProtocol::new(self.listen_protocol.clone(), ())
    }

    fn on_behaviour_event(&mut self, event: HandlerIn) {
        match event {
            HandlerIn::SendUnit(msg) => {
                self.send_queue.push_back(msg);
                // TODO(AndrewL): Wake up poll to send the message
            }
        }
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, (), Self::ToBehaviour>> {
        // TODO(AndrewL): Emit received messages from receive queue
        // TODO(AndrewL): Poll outbound substream to send messages
        // TODO(AndrewL): Poll inbound substream to receive messages
        Poll::Pending
    }

    fn on_connection_event(
        &mut self,
        _event: ConnectionEvent<'_, Self::InboundProtocol, Self::OutboundProtocol>,
    ) {
        // TODO(AndrewL): Handle FullyNegotiatedInbound
        // TODO(AndrewL): Handle FullyNegotiatedOutbound
        // TODO(AndrewL): Handle DialUpgradeError
    }
}
