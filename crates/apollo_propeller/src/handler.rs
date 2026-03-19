//! Connection handler for the Propeller protocol.

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

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
    StreamUpgradeError,
};
use libp2p::swarm::{Stream, SubstreamProtocol};
use prost::Message;
use tracing::{error, trace, warn};

use crate::config::Config;
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
/// Interval in milliseconds for logging dial upgrade warnings.
const DIAL_UPGRADE_WARNING_INTERVAL_MS: u64 = 1000;

/// Protocol Handler that manages substreams with a peer.
///
/// We use separate unidirectional substreams: outbound for sending and inbound for receiving.
// TODO(AndrewL): Add this to the specs.
pub struct Handler {
    /// Upgrade configuration for the propeller protocol.
    listen_protocol: PropellerProtocol,
    /// The long-lived outbound substreams.
    // TODO(AndrewL): make substream number dynamic using a Vec and limiting the number of
    // concurrent streams through yamux config, please consider the case where two peers have
    // different limits on the number of streams
    outbound_substream: [OutboundSubstreamState; CONCURRENT_STREAMS],
    /// The long-lived inbound substreams.
    inbound_substream: [Option<InboundSubstreamState>; CONCURRENT_STREAMS],
    /// Queue of messages to send.
    send_queue: VecDeque<ProtoUnit>,
    /// Queue of events to emit to the behaviour (received units, errors, etc.).
    ///
    /// Events are pushed here from various sources (inbound messages, connection errors) and
    /// drained as the highest priority in `poll_inner`.
    events_to_emit: VecDeque<HandlerOut>,
    /// Maximum wire message size for batching.
    max_wire_message_size: usize,
    /// Bounded channel for sending received units directly to the engine, bypassing the Swarm's
    /// event path. Provides back-pressure: when the channel is full, the handler stops reading
    /// from the network.
    // TODO(AndrewL): remove #[allow(dead_code)] once used
    #[allow(dead_code)]
    unit_sender: futures::channel::mpsc::Sender<PropellerUnit>,
    /// Units decoded from a wire batch that haven't been delivered yet.
    /// Holds at most one batch worth of units (bounded by `max_wire_message_size`).
    /// The handler only reads new batches from the wire when this buffer is empty.
    unsent_units: VecDeque<PropellerUnit>,
    /// The most recent waker from [`ConnectionHandler::poll`], used to wake the task when new
    /// messages are enqueued via [`on_behaviour_event`].
    waker: Option<Waker>,
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
    pub fn new(
        config: &Config,
        unit_sender: futures::channel::mpsc::Sender<PropellerUnit>,
    ) -> Self {
        let protocol =
            PropellerProtocol::new(config.stream_protocol.clone(), config.max_wire_message_size);
        Handler {
            listen_protocol: protocol,
            inbound_substream: std::array::from_fn(|_| None),
            outbound_substream: std::array::from_fn(|_| OutboundSubstreamState::Idle),
            send_queue: VecDeque::new(),
            events_to_emit: VecDeque::new(),
            max_wire_message_size: config.max_wire_message_size,
            unit_sender,
            unsent_units: VecDeque::new(),
            waker: None,
        }
    }

    /// Polls a single inbound substream for incoming messages.
    fn poll_single_inbound_substream(
        inbound_substream: &mut Option<InboundSubstreamState>,
        unsent_units: &mut VecDeque<PropellerUnit>,
        cx: &mut Context<'_>,
    ) {
        loop {
            match inbound_substream.take() {
                Some(InboundSubstreamState::WaitingInput(substream)) => {
                    if Self::poll_single_inbound_substream_waiting_input(
                        inbound_substream,
                        substream,
                        unsent_units,
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
                // No inbound substream exists for this slot; nothing to poll.
                None => break,
            }
        }
    }

    /// Polls a single inbound substream that is waiting for input.
    fn poll_single_inbound_substream_waiting_input(
        inbound_substream: &mut Option<InboundSubstreamState>,
        mut substream: Framed<Stream, PropellerCodec>,
        unsent_units: &mut VecDeque<PropellerUnit>,
        cx: &mut Context<'_>,
    ) -> ControlFlow<()> {
        match substream.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(batch))) => {
                *inbound_substream = Some(InboundSubstreamState::WaitingInput(substream));
                Self::handle_received_batch(batch, unsent_units);
                // Continue the loop in case there are more messages ready
                ControlFlow::Continue(())
            }
            Poll::Ready(Some(Err(error))) => {
                trace!("Failed to read from inbound stream: {error}");
                *inbound_substream = Some(InboundSubstreamState::Closing(substream));
                // Continue to close the substream
                ControlFlow::Continue(())
            }
            Poll::Ready(None) => {
                trace!("Inbound stream closed by remote");
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
                if let Err(err) = res {
                    trace!("Inbound substream error while closing: {err}");
                }
                *inbound_substream = None;
            }
            Poll::Pending => {
                *inbound_substream = Some(InboundSubstreamState::Closing(substream));
            }
        }
    }

    /// Decodes a received batch and buffers the units in `unsent_units` for delivery.
    fn handle_received_batch(batch: ProtoBatch, unsent_units: &mut VecDeque<PropellerUnit>) {
        for proto_unit in batch.batch {
            match PropellerUnit::try_from(proto_unit) {
                Ok(unit) => {
                    unsent_units.push_back(unit);
                }
                Err(e) => {
                    // TODO(AndrewL): Either remove this warning or make it once every N ms.
                    warn!("Failed to convert protobuf unit to unit: {e}");
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
        if batch.encoded_len() > max_wire_message_size {
            warn!("Propeller unit size exceeds max wire message size, sending will fail");
        }

        while let Some(msg) = send_queue.front() {
            batch.batch.push(msg.clone());
            if batch.encoded_len() <= max_wire_message_size {
                send_queue.pop_front();
            } else {
                batch.batch.pop();
                break;
            }
        }

        batch
    }

    fn on_fully_negotiated_inbound(&mut self, substream: Framed<Stream, PropellerCodec>) {
        trace!("New inbound substream request");
        let Some(index) = self.inbound_substream.iter().position(|s| s.is_none()) else {
            // TODO(AndrewL): Either remove this warning or make it once every N ms.
            warn!("No available slot for inbound substream");
            // In libp2p, dropping the Framed<Stream> sends a RST to the remote peer, which is
            // equivalent to rejecting the substream. No explicit reject API exists on the handler.
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
                warn!(
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
        // TODO(AndrewL): In poll_send, encountering an Idle slot with a non-empty send_queue causes
        // an immediate return, skipping all subsequent Active substreams in the array. Those Active
        // substreams won't get polled for flushing or sending. When CONCURRENT_STREAMS is increased
        // above 1 (which is the stated goal of this PR), a failed-and-reset substream at a lower
        // index will starve higher-indexed Active substreams, delaying message delivery and
        // preventing flushes of already-buffered data. Polling Active substreams before
        // transitioning Idle ones would avoid this.
        for (index, outbound_substream) in self.outbound_substream.iter_mut().enumerate() {
            // Only request an outbound substream when there are messages to send.
            // Without this guard, a DialUpgradeError (e.g. from an unsupported peer) resets state
            // to Idle, and the next poll would immediately request another substream — even with
            // an empty queue — causing infinite negotiation churn.
            match outbound_substream {
                OutboundSubstreamState::Idle => {
                    if self.send_queue.is_empty() {
                        continue;
                    }
                    *outbound_substream = OutboundSubstreamState::Pending;
                    return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                        protocol: SubstreamProtocol::new(self.listen_protocol.clone(), index),
                    });
                }
                OutboundSubstreamState::Pending => {}
                OutboundSubstreamState::Active { .. } => {
                    if let Some(event) = Self::poll_active_outbound_substream(
                        outbound_substream,
                        &mut self.send_queue,
                        self.max_wire_message_size,
                        cx,
                    ) {
                        return Poll::Ready(event);
                    }
                }
            }
        }

        Poll::Pending
    }

    /// Polls a single active outbound substream: flushes pending data and sends queued messages.
    ///
    /// Returns `Some(event)` if an error event should be emitted to the behaviour, or `None` if
    /// the substream was fully processed (flushed, pending, or nothing to send).
    // TODO(AndrewL): Consider holding the Pin::new(&mut substream) future inside
    // the state instead of recreating it every poll
    fn poll_active_outbound_substream(
        outbound_substream: &mut OutboundSubstreamState,
        send_queue: &mut VecDeque<ProtoUnit>,
        max_wire_message_size: usize,
        cx: &mut Context<'_>,
    ) -> Option<
        ConnectionHandlerEvent<
            <Handler as ConnectionHandler>::OutboundProtocol,
            <Handler as ConnectionHandler>::OutboundOpenInfo,
            <Handler as ConnectionHandler>::ToBehaviour,
        >,
    > {
        loop {
            let OutboundSubstreamState::Active { mut substream, mut should_flush } =
                std::mem::replace(outbound_substream, OutboundSubstreamState::Idle)
            else {
                // This function takes `&mut OutboundSubstreamState` rather than the Active fields
                // directly because it needs to replace the state entirely (e.g. back to Idle on
                // error, or updating Active with new should_flush).
                unreachable!("poll_active_outbound_substream called on non-Active substream");
            };

            if send_queue.is_empty() {
                // TODO(AndrewL): Extract a helper function to poll flush.
                if should_flush {
                    match Sink::poll_flush(Pin::new(&mut substream), cx) {
                        Poll::Ready(Ok(())) => {
                            should_flush = false;
                            *outbound_substream =
                                OutboundSubstreamState::Active { substream, should_flush };
                            continue;
                        }
                        Poll::Ready(Err(err)) => {
                            error!("Failed to flush outbound stream: {err}");
                            return Some(ConnectionHandlerEvent::NotifyBehaviour(
                                HandlerOut::SendError(err.to_string()),
                            ));
                        }
                        Poll::Pending => {
                            *outbound_substream =
                                OutboundSubstreamState::Active { substream, should_flush };
                            return None;
                        }
                    }
                } else {
                    *outbound_substream =
                        OutboundSubstreamState::Active { substream, should_flush };
                    return None;
                }
            }
            // TODO(AndrewL): extract a function here
            match Sink::poll_ready(Pin::new(&mut substream), cx) {
                Poll::Ready(Ok(())) => {
                    let message = Self::create_message_batch(send_queue, max_wire_message_size);
                    match Sink::start_send(Pin::new(&mut substream), message) {
                        Ok(()) => {
                            should_flush = true;
                            *outbound_substream =
                                OutboundSubstreamState::Active { substream, should_flush };
                            continue;
                        }
                        Err(err) => {
                            // TODO(AndrewL): Units were lost, consider a re-try mechanism.
                            error!("Failed to send message on outbound stream: {err}");
                            return Some(ConnectionHandlerEvent::NotifyBehaviour(
                                HandlerOut::SendError(err.to_string()),
                            ));
                        }
                    }
                }
                Poll::Ready(Err(err)) => {
                    // TODO(AndrewL): Units were lost, consider a re-try mechanism.
                    error!("Failed to send message on outbound stream: {err}");
                    return Some(ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::SendError(
                        err.to_string(),
                    )));
                }
                Poll::Pending => {
                    *outbound_substream =
                        OutboundSubstreamState::Active { substream, should_flush };
                    return None;
                }
            }
        }
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
            || self.events_to_emit.len() > QUEUE_WARNING_THRESHOLD
        {
            warn_every_n_ms!(
                QUEUE_WARNING_INTERVAL_MS,
                "Backlog in propeller handler. This indicates the peer is not consuming messages \
                 fast enough or the network is congested. Send queue length: {}, Events to emit \
                 queue length: {}",
                self.send_queue.len(),
                self.events_to_emit.len()
            );
        }

        // Drain any queued events first (errors from DialUpgradeError, etc.)
        if let Some(event) = self.events_to_emit.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(event));
        }

        // Deliver unsent units from a previously decoded batch via the behaviour path.
        // Units are drained one at a time so the Swarm can interleave other work.
        if let Some(unit) = self.unsent_units.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::Unit(unit)));
        }

        // Process outbound stream
        if let Poll::Ready(event) = self.poll_send(cx) {
            return Poll::Ready(event);
        }

        // Read from the wire only when the unsent buffer is empty (the previous batch has been
        // fully delivered). This ensures partially-delivered batches don't cause data loss.
        for inbound_substream in self.inbound_substream.iter_mut() {
            Self::poll_single_inbound_substream(inbound_substream, &mut self.unsent_units, cx);
        }

        // Deliver the first unit from the newly-read batch (if any).
        if let Some(unit) = self.unsent_units.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::Unit(unit)));
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
                if let Some(waker) = self.waker.as_ref() {
                    waker.wake_by_ref();
                }
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        let result = self.poll_inner(cx);
        // Store the waker so on_behaviour_event / on_connection_event can wake us.
        // Only clone when we're actually going to sleep (Pending), and skip if unchanged.
        if result.is_pending() && !self.waker.as_ref().is_some_and(|w| w.will_wake(cx.waker())) {
            self.waker = Some(cx.waker().clone());
        }
        result
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
                // TODO(AndrewL): Consider replacing the array+index mechanism with a Vec of
                // active substreams and a counter for the number of pending upgrades. On error,
                // decrement the counter; in poll, request new upgrades when the counter is below
                // the target.

                // Log the specific error type
                match &dial_upgrade_error.error {
                    StreamUpgradeError::Timeout => {
                        trace!("Dial upgrade error: Protocol negotiation timeout");
                    }
                    StreamUpgradeError::Apply(err) => {
                        // PropellerProtocol uses Infallible as its Error type, so Apply errors
                        // cannot occur
                        match *err {}
                    }
                    StreamUpgradeError::NegotiationFailed => {
                        warn_every_n_ms!(
                            DIAL_UPGRADE_WARNING_INTERVAL_MS,
                            "The remote peer does not support propeller on this connection"
                        );
                    }
                    StreamUpgradeError::Io(err) => {
                        trace!("Protocol negotiation failed: {err}");
                    }
                }

                // Reset the specific Pending substream to Idle so we can request a new one.
                let index = dial_upgrade_error.info;
                if matches!(self.outbound_substream[index], OutboundSubstreamState::Pending) {
                    self.outbound_substream[index] = OutboundSubstreamState::Idle;
                } else {
                    error!(
                        "Dial upgrade error but no pending substream found. (File a bug report if \
                         you see this)"
                    );
                }

                // Clear the send queue and report the failure to the behaviour. Without this,
                // messages would silently accumulate and the handler would enter an infinite
                // renegotiation loop (Idle → request → error → Idle → ...) against unsupported
                // peers.
                let dropped_count = self.send_queue.len();
                if dropped_count > 0 {
                    self.send_queue.clear();
                    self.events_to_emit.push_back(HandlerOut::SendError(format!(
                        "Dial upgrade failed, {dropped_count} queued message(s) lost"
                    )));
                }
                if let Some(waker) = self.waker.as_ref() {
                    waker.wake_by_ref();
                }
            }
            _ => {}
        }
    }
}
