use std::collections::HashMap;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use futures::stream::SelectAll;
use futures::StreamExt;
use libp2p::core::transport::PortUse;
use libp2p::core::Endpoint;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{
    dummy,
    ConnectionClosed,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use tokio::time::Instant;
use tokio_retry::strategy::ExponentialBackoff;
use tracing::debug;

use super::dial_peer::DialPeerStream;
use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

/// Minimum connection duration to be considered "genuine". Connections shorter than this are
/// treated as connect-then-reject cycles (e.g., the remote peer's connection_limits denying
/// because it still has a stale connection) and advance the reconnection backoff.
const MIN_CONNECTION_DURATION_FOR_BACKOFF_RESET: Duration = Duration::from_secs(1);

/// Manages dialing to a dynamic set of peers using explicit multiaddresses,
/// with exponential backoff on failure.
///
/// Each peer gets its own [`DialPeerStream`] that drives the dial lifecycle.
/// Streams terminate once a connection is established. This behaviour does not
/// re-dial peers after disconnection — callers must call
/// [`request_dial`](Self::request_dial) again if reconnection is desired.
///
/// The behaviour also tracks per-peer reconnection backoff that is independent of the
/// stream's internal dial-failure backoff. This prevents connect-then-reject cycles
/// (split-brain) from defeating the exponential backoff — even though each cycle creates
/// a new stream, the reconnection backoff state is preserved in the behaviour.
pub struct DialingBehaviour {
    retry_config: RetryConfig,
    peers: SelectAll<DialPeerStream>,
    /// Tracks when each connection was established, keyed by connection ID.
    /// Used to determine whether a `ConnectionClosed` is short-lived (split-brain)
    /// or long-lived (genuine disconnect).
    connection_timestamps: HashMap<ConnectionId, Instant>,
    /// Per-peer exponential backoff for reconnection after short-lived connections.
    /// Independent of the stream's internal dial-failure backoff.
    // TODO(AndrewL): entries are never removed for peers that are no longer dialed. Fine for
    // the small fixed set of bootstrap peers, but consider periodic cleanup if this is used
    // for dynamic peers.
    reconnect_backoffs: HashMap<PeerId, ExponentialBackoff>,
    /// Computed next-dial-time for peers that recently had a short-lived connection close.
    /// Consumed (removed) by `request_dial` when creating the next stream for this peer.
    pending_reconnect_delays: HashMap<PeerId, Instant>,
    waker: Option<Waker>,
}

impl DialingBehaviour {
    pub fn new(retry_config: RetryConfig) -> Self {
        Self {
            retry_config,
            peers: SelectAll::new(),
            connection_timestamps: HashMap::new(),
            reconnect_backoffs: HashMap::new(),
            pending_reconnect_delays: HashMap::new(),
            waker: None,
        }
    }

    /// Request dialing a peer at the given addresses.
    ///
    /// Creates a new [`DialPeerStream`] for the peer. If a stream for this peer already
    /// exists (pending or in-progress), it is cancelled and replaced.
    ///
    /// If the peer recently had a short-lived connection close (indicating a split-brain
    /// scenario), the new stream's first dial will be delayed according to the accumulated
    /// reconnection backoff.
    pub fn request_dial(&mut self, peer_id: PeerId, addresses: Vec<Multiaddr>) {
        self.cancel_dial(&peer_id);
        let mut stream = DialPeerStream::new(&self.retry_config, peer_id, addresses);
        if let Some(next_dial_time) = self.pending_reconnect_delays.remove(&peer_id) {
            stream.set_next_dial_time(next_dial_time);
        }
        self.peers.push(stream);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    /// Cancel any pending or in-progress dial for a peer.
    ///
    /// No waker wake is needed here because cancellation does not produce any events to poll.
    pub fn cancel_dial(&mut self, peer_id: &PeerId) {
        for stream in self.peers.iter_mut() {
            if stream.peer_id() == peer_id {
                stream.cancel();
            }
        }
    }
}

impl NetworkBehaviour for DialingBehaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = ToOtherBehaviourEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        // Forward events to all active streams (for their internal DialFailure handling).
        for stream in self.peers.iter_mut() {
            stream.on_swarm_event(event);
        }

        // Track per-peer connection state for reconnection backoff.
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                connection_id,
                endpoint,
                ..
            }) => {
                // Only track outbound connections (dials we initiated). Inbound connections
                // closing quickly are not split-brain dial issues.
                if endpoint.is_dialer() {
                    self.connection_timestamps.insert(connection_id, Instant::now());
                }
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                remaining_established,
                ..
            }) => {
                let timestamp = self.connection_timestamps.remove(&connection_id);
                if remaining_established > 0 {
                    return;
                }

                let is_short_lived = timestamp
                    .is_some_and(|t| t.elapsed() < MIN_CONNECTION_DURATION_FOR_BACKOFF_RESET);
                if is_short_lived {
                    let backoff = self
                        .reconnect_backoffs
                        .entry(peer_id)
                        .or_insert_with(|| self.retry_config.strategy());
                    let delay = backoff
                        .next()
                        .expect("A bounded ExponentialBackoff is an infinite iterator");
                    debug!(
                        ?peer_id,
                        ?delay,
                        "Short-lived connection closed, applying reconnect backoff"
                    );
                    self.pending_reconnect_delays.insert(peer_id, Instant::now() + delay);
                } else {
                    self.reconnect_backoffs.remove(&peer_id);
                    self.pending_reconnect_delays.remove(&peer_id);
                }
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        self.waker = Some(cx.waker().clone());
        match self.peers.poll_next_unpin(cx) {
            Poll::Ready(Some(event)) => Poll::Ready(event),
            Poll::Ready(None) | Poll::Pending => Poll::Pending,
        }
    }
}
