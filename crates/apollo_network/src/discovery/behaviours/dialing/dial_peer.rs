// TODO(AndrewL): add a max retry count or timeout so that dials don't retry forever for
// permanently unreachable peers.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use futures::Stream;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{
    dummy,
    ConnectionHandler,
    ConnectionId,
    DialError,
    DialFailure,
    FromSwarm,
    ToSwarm,
};
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
    cooldown: Duration,
    waker: Option<Waker>,
}

enum DialState {
    /// Waiting to dial (immediately or after backoff).
    PendingDial { sleeper: Pin<Box<Sleep>> },
    /// A dial attempt is in progress with the given connection id.
    Dialing(ConnectionId),
    /// Connection established, waiting for it to stabilize. If `request_redial` is called before
    /// the timer fires the stream transitions back to `PendingDial` with accumulated backoff. If
    /// the timer expires the connection is considered stable and the stream terminates.
    CooldownBeforeDeletion { connection_stable_sleeper: Pin<Box<Sleep>> },
    /// Terminal state — the stream was explicitly cancelled by the caller.
    Cancelled,
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
            cooldown: retry_config.cooldown_seconds,
            waker: None,
        }
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    /// Returns `true` if the stream was explicitly cancelled.
    pub fn is_cancelled(&self) -> bool {
        matches!(self.state, DialState::Cancelled)
    }

    /// Mark this stream for termination. The next poll will return `None`.
    pub fn cancel(&mut self) {
        self.state = DialState::Cancelled;
        self.wake();
    }

    pub fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. })
                if peer_id == self.peer_id && !self.is_cancelled() =>
            {
                self.state = DialState::CooldownBeforeDeletion {
                    connection_stable_sleeper: Box::pin(tokio::time::sleep_until(
                        Instant::now() + self.cooldown,
                    )),
                };
                self.wake();
            }
            FromSwarm::DialFailure(DialFailure {
                peer_id: Some(peer_id),
                connection_id,
                error,
                ..
            }) if peer_id == self.peer_id => {
                if !matches!(self.state, DialState::Dialing(id) if id == connection_id) {
                    return;
                }
                // The peer is already connected or being dialed — treat as established rather
                // than retrying in a loop against a peer we can already reach.
                if matches!(error, DialError::DialPeerConditionFalse(_)) {
                    self.state = DialState::CooldownBeforeDeletion {
                        connection_stable_sleeper: Box::pin(tokio::time::sleep_until(
                            Instant::now() + COOLDOWN,
                        )),
                    };
                    self.wake();
                    return;
                }
                self.schedule_retry();
            }
            _ => {}
        }
    }

    /// Re-request dialing this peer with updated addresses. Only takes effect if the stream is in
    /// `CooldownBeforeDeletion` (i.e., a connection was previously established). The next dial will
    /// use the accumulated backoff from the retry strategy.
    pub fn request_redial(&mut self, addresses: Vec<Multiaddr>) {
        self.addresses = addresses;
        if !matches!(self.state, DialState::CooldownBeforeDeletion { .. }) {
            return;
        }
        self.schedule_retry();
    }

    fn schedule_retry(&mut self) {
        let backoff = self
            .retry_strategy
            .next()
            .expect("A bounded ExponentialBackoff is an infinite iterator");
        self.state = DialState::PendingDial {
            sleeper: Box::pin(tokio::time::sleep_until(Instant::now() + backoff)),
        };
        debug!(?self.peer_id, ?backoff, "Scheduling retry");
        self.wake();
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
            DialState::Dialing(_) => Poll::Pending,
            DialState::PendingDial { sleeper } => match sleeper.as_mut().poll(cx) {
                Poll::Ready(()) => Poll::Ready(Some(self.emit_dial())),
                Poll::Pending => Poll::Pending,
            },
            DialState::Cancelled => Poll::Ready(None),
            DialState::CooldownBeforeDeletion { connection_stable_sleeper } => {
                match connection_stable_sleeper.as_mut().poll(cx) {
                    Poll::Ready(()) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}
