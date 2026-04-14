// TODO(AndrewL): add a max retry count or timeout so that dials don't retry forever for
// permanently unreachable peers.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures::Stream;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{dummy, ConnectionHandler, ConnectionId, DialFailure, FromSwarm, ToSwarm};
use libp2p::{Multiaddr, PeerId};
use tokio::time::{Instant, Sleep};
use tokio_retry::strategy::ExponentialBackoff;
use tracing::debug;

use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

/// A stream that drives a single peer's dial lifecycle with exponential backoff.
///
/// The stream emits `ToSwarm::Dial` events and terminates (`None`) once a
/// connection is established or the dial is cancelled.
pub struct DialPeerStream {
    peer_id: PeerId,
    addresses: Vec<Multiaddr>,
    state: DialState,
    retry_strategy: ExponentialBackoff,
    waker: Option<Waker>,
}

enum DialState {
    /// Waiting to dial (immediately or after backoff).
    PendingDial { sleeper: Pin<Box<Sleep>> },
    /// A dial attempt is in progress with the given connection id.
    Dialing(ConnectionId),
    /// Terminal state - connection was established after the request, no guarantee if it's still
    /// connected.
    Done,
}

impl DialPeerStream {
    pub fn new(retry_config: &RetryConfig, peer_id: PeerId, addresses: Vec<Multiaddr>) -> Self {
        Self {
            peer_id,
            addresses,
            state: DialState::PendingDial {
                sleeper: Box::pin(tokio::time::sleep_until(Instant::now())),
            },
            retry_strategy: retry_config.strategy(),
            waker: None,
        }
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    /// Mark this stream for termination. The next poll will return `None`.
    pub fn cancel(&mut self) {
        self.state = DialState::Done;
        self.wake();
    }

    pub fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. })
                if peer_id == self.peer_id =>
            {
                self.state = DialState::Done;
                self.wake();
            }
            FromSwarm::DialFailure(DialFailure {
                peer_id: Some(peer_id), connection_id, ..
            }) if peer_id == self.peer_id => {
                if !matches!(self.state, DialState::Dialing(id) if id == connection_id) {
                    return;
                }
                let backoff = self
                    .retry_strategy
                    .next()
                    .expect("A bounded ExponentialBackoff is an infinite iterator");
                self.state = DialState::PendingDial {
                    sleeper: Box::pin(tokio::time::sleep_until(Instant::now() + backoff)),
                };
                debug!(?self.peer_id, ?backoff, "Dial failed, scheduling retry");
                self.wake();
            }
            _ => {}
        }
    }

    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    fn emit_dial<T, W>(&mut self) -> ToSwarm<T, W> {
        let opts = DialOpts::peer_id(self.peer_id)
            .addresses(self.addresses.clone())
            .condition(PeerCondition::DisconnectedAndNotDialing)
            .build();
        self.state = DialState::Dialing(opts.connection_id());
        debug!(?self.peer_id, addresses = ?self.addresses, "Dialing peer");
        ToSwarm::Dial { opts }
    }
}

impl Stream for DialPeerStream {
    type Item = ToSwarm<
        ToOtherBehaviourEvent,
        <dummy::ConnectionHandler as ConnectionHandler>::FromBehaviour,
    >;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.waker = Some(cx.waker().clone());

        match &mut self.state {
            DialState::Done => Poll::Ready(None),
            DialState::Dialing(_) => Poll::Pending,
            DialState::PendingDial { sleeper } => match sleeper.as_mut().poll(cx) {
                Poll::Ready(()) => Poll::Ready(Some(self.emit_dial())),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}
