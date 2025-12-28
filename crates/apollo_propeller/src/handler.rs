//! Connection handler for the Propeller protocol.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use apollo_infra_utils::warn_every_n_sec;
use apollo_protobuf::protobuf::{PropellerUnit as ProtoUnit, PropellerUnitBatch as ProtoUnitBatch};
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

const CONCURRENT_STREAMS: usize = 1;

/// Protocol Handler that manages substreams with a peer.
pub struct Handler {
    /// Upgrade configuration for the propeller protocol.
    listen_protocol: PropellerProtocol,
    /// The single long-lived outbound substream.
    outbound_substream: [Option<OutboundSubstreamState>; CONCURRENT_STREAMS],
    /// Flag indicating that an outbound substream is being established to prevent duplicate
    /// requests.
    establishing_outbound_substream: [bool; CONCURRENT_STREAMS],
    /// The single long-lived inbound substream.
    inbound_substream: [Option<InboundSubstreamState>; CONCURRENT_STREAMS],
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

/// State of the outbound substream, opened either by us or by the remote.
struct OutboundSubstreamState {
    substream: Framed<Stream, PropellerCodec>,
    should_flush: bool,
}

impl Handler {
    /// Builds a new [`Handler`].
    pub fn new(stream_protocol: StreamProtocol, max_wire_message_size: usize) -> Self {
        let protocol = PropellerProtocol::new(stream_protocol, max_wire_message_size);
        Handler {
            listen_protocol: protocol,
            inbound_substream: (0..CONCURRENT_STREAMS)
                .map(|_| None)
                .collect::<Vec<_>>()
                .try_into()
                .map_err(|_| "Failed to convert Vec to array")
                .unwrap(),
            outbound_substream: (0..CONCURRENT_STREAMS)
                .map(|_| None)
                .collect::<Vec<_>>()
                .try_into()
                .map_err(|_| "Failed to convert Vec to array")
                .unwrap(),
            establishing_outbound_substream: [false; CONCURRENT_STREAMS],
            send_queue: VecDeque::new(),
            receive_queue: VecDeque::new(),
            max_wire_message_size,
        }
    }

    /// Create a batch of messages from the send queue that fits within max_wire_message_size.
    fn create_message_batch(
        send_queue: &mut VecDeque<ProtoUnit>,
        max_wire_message_size: usize,
    ) -> ProtoUnitBatch {
        if send_queue.is_empty() {
            return ProtoUnitBatch { batch: Vec::new() };
        }

        let max_batch_size = (max_wire_message_size * 9) / 10;
        let mut batch = Vec::new();
        let mut batch_size = 0;

        while let Some(msg) = send_queue.front() {
            let msg_size = msg.encoded_len();
            if batch.is_empty() || batch_size + msg_size <= max_batch_size {
                batch.push(send_queue.pop_front().unwrap());
                batch_size += msg_size;
            } else {
                break;
            }
        }

        ProtoUnitBatch { batch }
    }

    fn on_fully_negotiated_inbound(&mut self, substream: Framed<Stream, PropellerCodec>) {
        tracing::trace!("New inbound substream request");
        let Some(index) = self.inbound_substream.iter().position(|s| s.is_none()) else {
            tracing::error!("No available slot for inbound substream");
            return;
        };
        self.inbound_substream[index] = Some(InboundSubstreamState::WaitingInput(substream));
    }

    fn on_fully_negotiated_outbound(
        &mut self,
        FullyNegotiatedOutbound { protocol, .. }: FullyNegotiatedOutbound<
            <Handler as ConnectionHandler>::OutboundProtocol,
        >,
    ) {
        let substream = protocol;
        let index = self
            .outbound_substream
            .iter()
            .position(|s| s.is_none())
            .expect("No available slot for outbound substream");
        self.outbound_substream[index] =
            Some(OutboundSubstreamState { substream, should_flush: false });
        self.establishing_outbound_substream[index] = false;
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
        for (outbound_substream, is_establishing) in
            self.outbound_substream.iter_mut().zip(self.establishing_outbound_substream.iter_mut())
        {
            if outbound_substream.is_none() {
                if !*is_establishing {
                    *is_establishing = true;
                    return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                        protocol: SubstreamProtocol::new(self.listen_protocol.clone(), ()),
                    });
                }
                continue;
            }

            loop {
                let mut state = outbound_substream.take().unwrap();
                if self.send_queue.is_empty() {
                    // Queue is empty, maybe we just need to flush the stream
                    if state.should_flush {
                        match Sink::poll_flush(Pin::new(&mut state.substream), cx) {
                            Poll::Ready(Ok(())) => {
                                state.should_flush = false;
                                *outbound_substream = Some(state);
                                continue;
                            }
                            Poll::Ready(Err(e)) => {
                                tracing::error!("Failed to flush outbound stream: {e}");
                                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                                    HandlerOut::SendError(e.to_string()),
                                ));
                            }
                            Poll::Pending => {
                                *outbound_substream = Some(state);
                                break;
                            }
                        }
                    } else {
                        *outbound_substream = Some(state);
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
                                    *outbound_substream = Some(state);
                                    continue;
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to send message on outbound stream: {e}"
                                    );
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
                            *outbound_substream = Some(state);
                            break;
                        }
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
        if self.send_queue.len() > 100 || self.receive_queue.len() > 100 {
            warn_every_n_sec!(
                1,
                "Send queue length: {}, Receive queue length: {}",
                self.send_queue.len(),
                self.receive_queue.len()
            );
        }

        // First, emit any queued received messages
        if let Some(message) = self.receive_queue.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::Unit(message)));
        }

        // Process outbound stream
        if let Poll::Ready(event) = self.poll_send(cx) {
            return Poll::Ready(event);
        }

        // Handle inbound messages
        for inbound_substream in self.inbound_substream.iter_mut() {
            loop {
                match inbound_substream.take() {
                    Some(InboundSubstreamState::WaitingInput(mut substream)) => {
                        match substream.poll_next_unpin(cx) {
                            Poll::Ready(Some(Ok(messages))) => {
                                *inbound_substream =
                                    Some(InboundSubstreamState::WaitingInput(substream));
                                // Add all received messages to the queue
                                for proto_unit in messages.batch {
                                    match PropellerUnit::try_from(proto_unit) {
                                        Ok(unit) => {
                                            self.receive_queue.push_back(unit);
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                "Failed to convert protobuf unit to unit: {e}"
                                            );
                                        }
                                    }
                                }
                                // Wake up to emit the first message
                                cx.waker().wake_by_ref();
                                break;
                            }
                            Poll::Ready(Some(Err(error))) => {
                                tracing::trace!("Failed to read from inbound stream: {error}");
                                // Close this side of the stream. If the
                                // peer is still around, they will re-establish their
                                // outbound stream i.e. our inbound stream.
                                *inbound_substream =
                                    Some(InboundSubstreamState::Closing(substream));
                            }
                            // peer closed the stream
                            Poll::Ready(None) => {
                                tracing::trace!("Inbound stream closed by remote");
                                *inbound_substream =
                                    Some(InboundSubstreamState::Closing(substream));
                            }
                            Poll::Pending => {
                                *inbound_substream =
                                    Some(InboundSubstreamState::WaitingInput(substream));
                                break;
                            }
                        }
                    }
                    Some(InboundSubstreamState::Closing(mut substream)) => {
                        match Sink::poll_close(Pin::new(&mut substream), cx) {
                            Poll::Ready(res) => {
                                if let Err(e) = res {
                                    // Don't close the connection but just drop the inbound
                                    // substream. In case the
                                    // remote has more to send, they will open up a new
                                    // substream.
                                    tracing::trace!("Inbound substream error while closing: {e}");
                                }
                                *inbound_substream = None;
                                break;
                            }
                            Poll::Pending => {
                                *inbound_substream =
                                    Some(InboundSubstreamState::Closing(substream));
                                break;
                            }
                        }
                    }
                    None => {
                        *inbound_substream = None;
                        break;
                    }
                }
            }
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
