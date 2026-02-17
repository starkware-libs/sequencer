//! Connection handler for the Propeller protocol.

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::task::{Context, Poll};

use apollo_protobuf::protobuf::{PropellerUnit as ProtoUnit, PropellerUnitBatch as ProtoBatch};
use asynchronous_codec::Framed;
use futures::prelude::*;
use libp2p::swarm::handler::{
    ConnectionEvent,
    ConnectionHandler,
    ConnectionHandlerEvent,
    FullyNegotiatedInbound,
};
use libp2p::swarm::{Stream, StreamProtocol, SubstreamProtocol};

use crate::protocol::{PropellerCodec, PropellerProtocol};
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
    /// The single long-lived inbound substream.
    inbound_substream: Option<InboundSubstreamState>,
    /// Queue of messages to send.
    send_queue: VecDeque<ProtoUnit>,
    /// Queue of received messages to emit.
    receive_queue: VecDeque<PropellerUnit>,
}

/// State of the inbound substream, opened either by us or by the remote.
enum InboundSubstreamState {
    /// Waiting for a message from the remote. The idle state for an inbound substream.
    WaitingInput(Framed<Stream, PropellerCodec>),
    /// The substream is being closed.
    Closing(Framed<Stream, PropellerCodec>),
}

impl Handler {
    /// Builds a new [`Handler`].
    pub fn new(stream_protocol: StreamProtocol, max_wire_message_size: usize) -> Self {
        let protocol = PropellerProtocol::new(stream_protocol, max_wire_message_size);
        Handler {
            listen_protocol: protocol,
            inbound_substream: None,
            send_queue: VecDeque::new(),
            receive_queue: VecDeque::new(),
        }
    }

    /// Polls the inbound substream for incoming messages.
    fn poll_inbound_substream(&mut self, cx: &mut Context<'_>) {
        loop {
            // TODO(AndrewL): reduce code duplication with SQMR
            match self.inbound_substream.take() {
                Some(InboundSubstreamState::WaitingInput(substream)) => {
                    if self.poll_waiting_input(substream, cx).is_break() {
                        break;
                    }
                }
                Some(InboundSubstreamState::Closing(substream)) => {
                    self.progress_closing_substream(substream, cx);
                    break;
                }
                None => break,
            }
        }
    }

    /// Polls a substream waiting for input.
    fn poll_waiting_input(
        &mut self,
        mut substream: Framed<Stream, PropellerCodec>,
        cx: &mut Context<'_>,
    ) -> ControlFlow<()> {
        match substream.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(batch))) => {
                self.inbound_substream = Some(InboundSubstreamState::WaitingInput(substream));
                self.handle_received_batch(batch);
                // Continue the loop in case there are more messages ready
                ControlFlow::Continue(())
            }
            Poll::Ready(Some(Err(error))) => {
                tracing::warn!("Failed to read from inbound stream: {error}");
                self.inbound_substream = Some(InboundSubstreamState::Closing(substream));
                // Continue to close the substream
                ControlFlow::Continue(())
            }
            Poll::Ready(None) => {
                tracing::trace!("Inbound stream closed by remote");
                self.inbound_substream = Some(InboundSubstreamState::Closing(substream));
                // Continue to close the substream
                ControlFlow::Continue(())
            }
            Poll::Pending => {
                self.inbound_substream = Some(InboundSubstreamState::WaitingInput(substream));
                ControlFlow::Break(())
            }
        }
    }

    /// Polls a closing substream.
    fn progress_closing_substream(
        &mut self,
        mut substream: Framed<Stream, PropellerCodec>,
        cx: &mut Context<'_>,
    ) {
        match Sink::poll_close(Pin::new(&mut substream), cx) {
            Poll::Ready(res) => {
                if let Err(e) = res {
                    tracing::trace!("Inbound substream error while closing: {e}");
                }
                self.inbound_substream = None;
            }
            Poll::Pending => {
                self.inbound_substream = Some(InboundSubstreamState::Closing(substream));
            }
        }
    }

    /// Handles a received batch of units.
    fn handle_received_batch(&mut self, batch: ProtoBatch) {
        for proto_unit in batch.batch {
            match PropellerUnit::try_from(proto_unit) {
                Ok(unit) => {
                    self.receive_queue.push_back(unit);
                }
                Err(e) => {
                    // TODO(AndrewL): Either remove this warning or make it once every N ms.
                    tracing::warn!("Failed to convert protobuf unit to unit: {e}");
                }
            }
        }
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
                self.send_queue.push_back(msg.into());
                // TODO(AndrewL): Wake up poll to send the message
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, (), Self::ToBehaviour>> {
        // Emit received messages from receive queue
        if let Some(message) = self.receive_queue.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::Unit(message)));
        }

        // TODO(AndrewL): Poll outbound substream to send messages

        // Poll inbound substream to receive messages
        self.poll_inbound_substream(cx);

        // Check receive queue again after polling inbound substream
        if let Some(message) = self.receive_queue.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::Unit(message)));
        }

        Poll::Pending
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<'_, Self::InboundProtocol, Self::OutboundProtocol>,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol, ..
            }) => {
                if self.inbound_substream.is_some() {
                    // TODO(AndrewL): Either remove this warning or make it once every N ms.
                    tracing::warn!(
                        "Received new inbound substream but one already exists, replacing"
                    );
                }
                tracing::trace!("New inbound substream established");
                self.inbound_substream = Some(InboundSubstreamState::WaitingInput(protocol));
            }
            _ => {
                // TODO(AndrewL): Handle FullyNegotiatedOutbound
                // TODO(AndrewL): Handle DialUpgradeError
            }
        }
    }
}
