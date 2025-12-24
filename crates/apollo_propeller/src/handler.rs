//! Connection handler for the Propeller protocol.

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::task::{Context, Poll};

use apollo_infra_utils::warn_every_n_ms;
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
use tracing::warn;

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

/// Queue length threshold for logging warnings.
const QUEUE_WARNING_THRESHOLD: usize = 100;
/// Interval in milliseconds for logging queue warnings.
const QUEUE_WARNING_INTERVAL_MS: u64 = 1000;

/// Protocol Handler that manages substreams with a peer.
///
/// We use separate unidirectional substreams: outbound for sending and inbound for receiving.
// TODO(AndrewL): Add this to the specs.
pub struct Handler {
    /// Upgrade configuration for the propeller protocol.
    listen_protocol: PropellerProtocol,
    /// The long-lived outbound substreams.
    // TODO(AndrewL): make substream number dynamic using a Vec, please consider the case where two
    // peers have different limits on the number of streams
    outbound_substream: [OutboundSubstreamState; CONCURRENT_STREAMS],
    /// The long-lived inbound substreams.
    inbound_substream: [Option<InboundSubstreamState>; CONCURRENT_STREAMS],
    /// Queue of messages to send.
    send_queue: VecDeque<ProtoUnit>,
    /// Queue of received messages to emit.
    receive_queue: VecDeque<PropellerUnit>,
    /// Maximum wire message size for batching.
    max_wire_message_size: usize,
}

/// State of the inbound substream, opened by the remote peer.
#[derive(Debug)]
enum InboundSubstreamState {
    /// Waiting for a message from the remote. The idle state for an inbound substream.
    WaitingInput(Framed<Stream, PropellerCodec>),
    /// The substream is being closed.
    Closing(Framed<Stream, PropellerCodec>),
}

/// State of the outbound substream, opened by us.
#[derive(Debug)]
enum OutboundSubstreamState {
    /// No substream exists and no request is pending.
    Idle,
    /// A substream request has been sent but not yet negotiated.
    Pending,
    /// The substream is active and ready to send messages.
    Active {
        substream: Framed<Stream, PropellerCodec>,
        /// True if we've sent data and need to flush the stream.
        ///
        /// In this implementation we use flush to ensure that the data was actually sent. Without
        /// flushing periodically we cannot be certain what was sent and what failed when a failure
        /// occurs.
        // TODO(AndrewL): Discuss not flushing
        should_flush: bool,
    },
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
                .expect("Vec length matches CONCURRENT_STREAMS constant, conversion cannot fail"),
            outbound_substream: (0..CONCURRENT_STREAMS)
                .map(|_| OutboundSubstreamState::Idle)
                .collect::<Vec<_>>()
                .try_into()
                .expect("Vec length matches CONCURRENT_STREAMS constant, conversion cannot fail"),
            send_queue: VecDeque::new(),
            receive_queue: VecDeque::new(),
            max_wire_message_size,
        }
    }

    /// Polls a single inbound substream for incoming messages.
    fn poll_single_inbound_substream(
        inbound_substream: &mut Option<InboundSubstreamState>,
        receive_queue: &mut VecDeque<PropellerUnit>,
        cx: &mut Context<'_>,
    ) {
        loop {
            match inbound_substream.take() {
                Some(InboundSubstreamState::WaitingInput(substream)) => {
                    if Self::poll_single_inbound_substream_waiting_input(
                        inbound_substream,
                        substream,
                        receive_queue,
                        cx,
                    )
                    .is_break()
                    {
                        break;
                    }
                }
                Some(InboundSubstreamState::Closing(substream)) => {
                    Self::poll_single_inbound_substream_closing(inbound_substream, substream, cx);
                    break;
                }
                None => break,
            }
        }
    }

    /// Polls a single inbound substream that is waiting for input.
    fn poll_single_inbound_substream_waiting_input(
        inbound_substream: &mut Option<InboundSubstreamState>,
        mut substream: Framed<Stream, PropellerCodec>,
        receive_queue: &mut VecDeque<PropellerUnit>,
        cx: &mut Context<'_>,
    ) -> ControlFlow<()> {
        match substream.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(batch))) => {
                *inbound_substream = Some(InboundSubstreamState::WaitingInput(substream));
                Self::handle_received_batch(batch, receive_queue);
                // Continue the loop in case there are more messages ready
                ControlFlow::Continue(())
            }
            Poll::Ready(Some(Err(error))) => {
                tracing::trace!("Failed to read from inbound stream: {error}");
                *inbound_substream = Some(InboundSubstreamState::Closing(substream));
                // Continue to close the substream
                ControlFlow::Continue(())
            }
            Poll::Ready(None) => {
                tracing::trace!("Inbound stream closed by remote");
                *inbound_substream = Some(InboundSubstreamState::Closing(substream));
                // Continue to close the substream
                ControlFlow::Continue(())
            }
            Poll::Pending => {
                *inbound_substream = Some(InboundSubstreamState::WaitingInput(substream));
                ControlFlow::Break(())
            }
        }
    }

    /// Polls a single inbound substream that is closing.
    fn poll_single_inbound_substream_closing(
        inbound_substream: &mut Option<InboundSubstreamState>,
        mut substream: Framed<Stream, PropellerCodec>,
        cx: &mut Context<'_>,
    ) {
        match Sink::poll_close(Pin::new(&mut substream), cx) {
            Poll::Ready(res) => {
                if let Err(e) = res {
                    tracing::trace!("Inbound substream error while closing: {e}");
                }
                *inbound_substream = None;
            }
            Poll::Pending => {
                *inbound_substream = Some(InboundSubstreamState::Closing(substream));
            }
        }
    }

    /// Handles a received batch of units.
    fn handle_received_batch(batch: ProtoBatch, receive_queue: &mut VecDeque<PropellerUnit>) {
        for proto_unit in batch.batch {
            match PropellerUnit::try_from(proto_unit) {
                Ok(unit) => {
                    receive_queue.push_back(unit);
                }
                Err(e) => {
                    // TODO(AndrewL): Either remove this warning or make it once every N ms.
                    tracing::warn!("Failed to convert protobuf unit to unit: {e}");
                }
            }
        }
    }

    /// Create a batch of messages from the send queue that fits within max_wire_message_size.
    fn create_message_batch(
        send_queue: &mut VecDeque<ProtoUnit>,
        max_wire_message_size: usize,
    ) -> ProtoBatch {
        if send_queue.is_empty() {
            return ProtoBatch { batch: Vec::new() };
        }

        let mut batch = ProtoBatch { batch: vec![send_queue.pop_front().unwrap()] };
        if batch.encoded_len() >= max_wire_message_size {
            warn!("Propeller unit size exceeds max wire message size, sending will fail");
        }

        while let Some(msg) = send_queue.front() {
            batch.batch.push(msg.clone());
            if batch.encoded_len() < max_wire_message_size {
                send_queue.pop_front();
            } else {
                batch.batch.pop();
                break;
            }
        }

        batch
    }

    fn on_fully_negotiated_inbound(&mut self, substream: Framed<Stream, PropellerCodec>) {
        tracing::trace!("New inbound substream request");
        let Some(index) = self.inbound_substream.iter().position(|s| s.is_none()) else {
            // TODO(AndrewL): Either remove this warning or make it once every N ms.
            tracing::warn!("No available slot for inbound substream");
            // Dropping the substream will close it, signaling rejection to the remote peer
            return;
        };
        // TODO(AndrewL): Check what happens with a malicious peer (maybe shouldn't overwrite the
        // existing substream?)
        self.inbound_substream[index] = Some(InboundSubstreamState::WaitingInput(substream));
    }

    fn on_fully_negotiated_outbound(
        &mut self,
        FullyNegotiatedOutbound { protocol, info: index }: FullyNegotiatedOutbound<
            <Handler as ConnectionHandler>::OutboundProtocol,
            <Handler as ConnectionHandler>::OutboundOpenInfo,
        >,
    ) {
        let substream = protocol;

        if let OutboundSubstreamState::Active { should_flush, .. } = &self.outbound_substream[index]
        {
            if *should_flush {
                tracing::warn!(
                    "New outbound substream while existing substream has pending data, data may \
                     be lost"
                );
            }
        }

        self.outbound_substream[index] =
            OutboundSubstreamState::Active { substream, should_flush: false };
    }

    fn poll_send(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<
            <Handler as ConnectionHandler>::OutboundProtocol,
            <Handler as ConnectionHandler>::OutboundOpenInfo,
            <Handler as ConnectionHandler>::ToBehaviour,
        >,
    > {
        for (index, outbound_substream) in self.outbound_substream.iter_mut().enumerate() {
            // No need to optimize requesting a sub-stream only when there are units to send since
            // the streams are long-lived (We only open a sub-stream twice per connection assuming
            // no errors occur). If we don't have an outbound substream, request one
            match outbound_substream {
                OutboundSubstreamState::Idle => {
                    *outbound_substream = OutboundSubstreamState::Pending;
                    return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                        protocol: SubstreamProtocol::new(self.listen_protocol.clone(), index),
                    });
                }
                OutboundSubstreamState::Pending => {
                    // Waiting for substream negotiation to complete
                    continue;
                }
                OutboundSubstreamState::Active { .. } => {
                    // Continue to process the active substream below
                }
            }

            loop {
                let OutboundSubstreamState::Active { mut substream, mut should_flush } =
                    std::mem::replace(outbound_substream, OutboundSubstreamState::Idle)
                else {
                    unreachable!("outbound_substream is Active at the start of this loop");
                };

                if self.send_queue.is_empty() {
                    // Queue is empty, maybe we just need to flush the stream
                    if should_flush {
                        match Sink::poll_flush(Pin::new(&mut substream), cx) {
                            Poll::Ready(Ok(())) => {
                                should_flush = false;
                                *outbound_substream =
                                    OutboundSubstreamState::Active { substream, should_flush };
                                continue;
                            }
                            Poll::Ready(Err(e)) => {
                                tracing::error!("Failed to flush outbound stream: {e}");
                                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                                    HandlerOut::SendError(e.to_string()),
                                ));
                            }
                            Poll::Pending => {
                                *outbound_substream =
                                    OutboundSubstreamState::Active { substream, should_flush };
                                break;
                            }
                        }
                    } else {
                        *outbound_substream =
                            OutboundSubstreamState::Active { substream, should_flush };
                        break;
                    }
                } else {
                    match Sink::poll_ready(Pin::new(&mut substream), cx) {
                        Poll::Ready(Ok(())) => {
                            let message = Self::create_message_batch(
                                &mut self.send_queue,
                                self.max_wire_message_size,
                            );
                            match Sink::start_send(Pin::new(&mut substream), message) {
                                Ok(()) => {
                                    // Try sending more messages if there are any
                                    should_flush = true;
                                    *outbound_substream =
                                        OutboundSubstreamState::Active { substream, should_flush };
                                    continue;
                                }
                                Err(e) => {
                                    // TODO(AndrewL): Units were lost, consider a re-try mechanism.
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
                            // TODO(AndrewL): Units were lost, consider a re-try mechanism.
                            tracing::error!("Failed to send message on outbound stream: {e}");
                            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                                HandlerOut::SendError(e.to_string()),
                            ));
                        }
                        Poll::Pending => {
                            // Not ready to send more messages yet
                            *outbound_substream =
                                OutboundSubstreamState::Active { substream, should_flush };
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
            <Handler as ConnectionHandler>::OutboundOpenInfo,
            <Handler as ConnectionHandler>::ToBehaviour,
        >,
    > {
        if self.send_queue.len() > QUEUE_WARNING_THRESHOLD
            || self.receive_queue.len() > QUEUE_WARNING_THRESHOLD
        {
            warn_every_n_ms!(
                QUEUE_WARNING_INTERVAL_MS,
                "Backlog in propeller handler. This indicates the peer is not consuming messages \
                 fast enough or the network is congested. Send queue length: {}, Receive queue \
                 length: {}",
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
            Self::poll_single_inbound_substream(inbound_substream, &mut self.receive_queue, cx);
        }

        // Check receive queue again after polling inbound substreams
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
    type OutboundOpenInfo = usize;
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
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        // TODO(AndrewL): inline this function into poll
        self.poll_inner(cx)
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            '_,
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol, ..
            }) => self.on_fully_negotiated_inbound(protocol),
            ConnectionEvent::FullyNegotiatedOutbound(fully_negotiated_outbound) => {
                self.on_fully_negotiated_outbound(fully_negotiated_outbound)
            }
            ConnectionEvent::DialUpgradeError(dial_upgrade_error) => {
                // Reset the specific Pending substream to Idle so we can request a new one
                let index = dial_upgrade_error.info;
                if matches!(self.outbound_substream[index], OutboundSubstreamState::Pending) {
                    self.outbound_substream[index] = OutboundSubstreamState::Idle;
                } else {
                    tracing::error!(
                        "Dial upgrade error but no pending substream found. (File a bug report if \
                         you see this)"
                    );
                }
            }
            _ => {}
        }
    }
}
