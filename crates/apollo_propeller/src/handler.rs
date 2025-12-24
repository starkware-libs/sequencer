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
    FullyNegotiatedOutbound,
};
use libp2p::swarm::{Stream, StreamProtocol, SubstreamProtocol};
use prost::Message;

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
    /// The single long-lived outbound substream.
    outbound_substream: Option<OutboundSubstreamState>,
    /// Queue of messages to send.
    send_queue: VecDeque<ProtoUnit>,
    /// Queue of received messages to emit.
    receive_queue: VecDeque<PropellerUnit>,
    /// Maximum wire message size for batching.
    max_wire_message_size: usize,
}

/// State of the inbound substream, opened either by us or by the remote.
enum InboundSubstreamState {
    /// Waiting for a message from the remote. The idle state for an inbound substream.
    WaitingInput(Framed<Stream, PropellerCodec>),
    /// The substream is being closed.
    Closing(Framed<Stream, PropellerCodec>),
}

/// State of the outbound substream, opened by us.
struct OutboundSubstreamState {
    substream: Framed<Stream, PropellerCodec>,
    /// True if we've sent data and need to flush the stream.
    should_flush: bool,
}

impl Handler {
    /// Builds a new [`Handler`].
    pub fn new(stream_protocol: StreamProtocol, max_wire_message_size: usize) -> Self {
        let protocol = PropellerProtocol::new(stream_protocol, max_wire_message_size);
        Handler {
            listen_protocol: protocol,
            inbound_substream: None,
            outbound_substream: None,
            send_queue: VecDeque::new(),
            receive_queue: VecDeque::new(),
            max_wire_message_size,
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

    /// Create a batch of messages from the send queue that fits within max_wire_message_size.
    ///
    /// Batching units together reduces overhead by sending multiple units in a single network
    /// message. While each unit represents a different message shard, batching them amortizes
    /// the per-message framing overhead and improves throughput, especially under high load.
    // TODO(AndrewL): Add a test for this function that tests that a batch doesn't exceed the
    // max_wire_message_size.
    fn create_message_batch(
        send_queue: &mut VecDeque<ProtoUnit>,
        max_wire_message_size: usize,
    ) -> ProtoBatch {
        if send_queue.is_empty() {
            return ProtoBatch { batch: Vec::new() };
        }

        let mut batch = Vec::new();
        let mut batch_size = 0;

        while let Some(msg) = send_queue.front() {
            let msg_size = msg.encoded_len();
            if batch.is_empty() || batch_size + msg_size <= max_wire_message_size {
                batch.push(send_queue.pop_front().unwrap());
                batch_size += msg_size;
            } else {
                break;
            }
        }

        ProtoBatch { batch }
    }

    fn on_fully_negotiated_inbound(&mut self, substream: Framed<Stream, PropellerCodec>) {
        if self.inbound_substream.is_some() {
            // TODO(AndrewL): Either remove this warning or make it once every N ms.
            tracing::warn!("Received new inbound substream but one already exists, replacing");
        }
        tracing::trace!("New inbound substream established");
        self.inbound_substream = Some(InboundSubstreamState::WaitingInput(substream));
    }

    fn on_fully_negotiated_outbound(
        &mut self,
        fully_negotiated_outbound: FullyNegotiatedOutbound<
            <Handler as ConnectionHandler>::OutboundProtocol,
        >,
    ) {
        let substream = fully_negotiated_outbound.protocol;
        self.outbound_substream = Some(OutboundSubstreamState { substream, should_flush: false });
    }

    fn poll_send(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<
            <Handler as ConnectionHandler>::OutboundProtocol,
            (),
            <Handler as ConnectionHandler>::ToBehaviour,
        >,
    > {
        // If we don't have an outbound substream, request one
        if self.outbound_substream.is_none() {
            return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(self.listen_protocol.clone(), ()),
            });
        }

        loop {
            let mut state = self.outbound_substream.take().expect(
                "outbound_substream is Some at the start of this function and at each potential \
                 end of this loop",
            );
            if self.send_queue.is_empty() {
                // Queue is empty, maybe we just need to flush the stream
                if state.should_flush {
                    match Sink::poll_flush(Pin::new(&mut state.substream), cx) {
                        Poll::Ready(Ok(())) => {
                            state.should_flush = false;
                            self.outbound_substream = Some(state);
                            continue;
                        }
                        Poll::Ready(Err(e)) => {
                            tracing::error!("Failed to flush outbound stream: {e}");
                            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                                HandlerOut::SendError(e.to_string()),
                            ));
                        }
                        Poll::Pending => {
                            self.outbound_substream = Some(state);
                            break;
                        }
                    }
                } else {
                    self.outbound_substream = Some(state);
                    break;
                }
            } else {
                match Sink::poll_ready(Pin::new(&mut state.substream), cx) {
                    Poll::Ready(Ok(())) => {
                        let message = Self::create_message_batch(
                            &mut self.send_queue,
                            self.max_wire_message_size,
                        );
                        match Sink::start_send(Pin::new(&mut state.substream), message) {
                            Ok(()) => {
                                // Try sending more messages if there are any
                                state.should_flush = true;
                                self.outbound_substream = Some(state);
                                continue;
                            }
                            Err(e) => {
                                tracing::error!("Failed to send message on outbound stream: {e}");
                                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                                    HandlerOut::SendError(e.to_string()),
                                ));
                            }
                        }
                    }
                    Poll::Ready(Err(e)) => {
                        tracing::error!("Failed to send message on outbound stream: {e}");
                        return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                            HandlerOut::SendError(e.to_string()),
                        ));
                    }
                    Poll::Pending => {
                        // Not ready to send more messages yet
                        self.outbound_substream = Some(state);
                        break;
                    }
                }
            }
        }

        Poll::Pending
    }

    fn poll_inner(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<
            <Handler as ConnectionHandler>::OutboundProtocol,
            (),
            <Handler as ConnectionHandler>::ToBehaviour,
        >,
    > {
        // First, emit any queued received messages
        if let Some(message) = self.receive_queue.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::Unit(message)));
        }

        // Process outbound stream
        if let Poll::Ready(event) = self.poll_send(cx) {
            return Poll::Ready(event);
        }

        // Poll inbound substream to receive messages
        self.poll_inbound_substream(cx);

        // Check receive queue again after polling inbound substream
        if let Some(message) = self.receive_queue.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::Unit(message)));
        }

        Poll::Pending
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
        self.poll_inner(cx)
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<'_, Self::InboundProtocol, Self::OutboundProtocol>,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol, ..
            }) => self.on_fully_negotiated_inbound(protocol),
            ConnectionEvent::FullyNegotiatedOutbound(fully_negotiated_outbound) => {
                self.on_fully_negotiated_outbound(fully_negotiated_outbound)
            }
            _ => {
                // TODO(AndrewL): Handle DialUpgradeError variants
            }
        }
    }
}
