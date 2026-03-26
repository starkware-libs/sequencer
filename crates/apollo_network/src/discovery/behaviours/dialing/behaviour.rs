use std::task::{Context, Poll, Waker};

use futures::stream::SelectAll;
use futures::StreamExt;
use libp2p::core::transport::PortUse;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    dummy,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};

use super::dial_peer::DialPeerStream;
use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

/// Manages dialing to a dynamic set of peers using explicit multiaddresses,
/// with exponential backoff on failure.
///
/// Each peer gets its own [`DialPeerStream`] that drives the dial lifecycle.
/// Streams terminate once a connection is established. This behaviour does not
/// re-dial peers after disconnection — callers must call
/// [`request_dial`](Self::request_dial) again if reconnection is desired.
pub struct DialingBehaviour {
    retry_config: RetryConfig,
    peers: SelectAll<DialPeerStream>,
    waker: Option<Waker>,
}

impl DialingBehaviour {
    pub fn new(retry_config: RetryConfig) -> Self {
        Self { retry_config, peers: SelectAll::new(), waker: None }
    }

    /// Request dialing a peer at the given addresses.
    ///
    /// Creates a new [`DialPeerStream`] for the peer. If an active stream already exists, updates
    /// its addresses and requests a redial.
    pub fn request_dial(&mut self, peer_id: PeerId, addresses: Vec<Multiaddr>) {
        let active_stream =
            self.peers.iter_mut().find(|s| *s.peer_id() == peer_id && !s.is_cancelled());
        match active_stream {
            None => {
                self.peers.push(DialPeerStream::new(&self.retry_config, peer_id, addresses));
            }
            Some(stream) => stream.request_redial(addresses),
        }
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
        for stream in self.peers.iter_mut() {
            stream.on_swarm_event(event);
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
