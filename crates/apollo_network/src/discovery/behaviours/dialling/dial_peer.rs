use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures::Stream;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{dummy, ConnectionHandler, DialFailure, FromSwarm, ToSwarm};
use libp2p::{Multiaddr, PeerId};
use tokio::time::{Instant, Sleep};
use tokio_retry::strategy::ExponentialBackoff;
use tracing::debug;

use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

/// A stream that drives a single peer's dial lifecycle with exponential backoff.
///
/// The stream emits `ToSwarm::Dial` events and terminates (`None`) once a
/// connection is established or the dial is cancelled.
// TODO(AndrewL): remove this once the behaviour is added
#[allow(dead_code)]
pub struct DialPeerStream {
    peer_id: PeerId,
    addresses: Vec<Multiaddr>,
    state: DialState,
    retry_strategy: ExponentialBackoff,
    next_dial_time: Instant,
    waker: Option<Waker>,
    sleeper: Option<Pin<Box<Sleep>>>,
}

#[derive(Debug, PartialEq, Eq)]
enum DialState {
    /// Waiting to dial (immediately or after backoff).
    PendingDial,
    /// A dial attempt is in progress.
    Dialing,
    /// Terminal state — connection established or cancelled.
    Done,
}

// TODO(AndrewL): remove this once the behaviour is added
#[allow(dead_code)]
impl DialPeerStream {
    pub fn new(retry_config: &RetryConfig, peer_id: PeerId, addresses: Vec<Multiaddr>) -> Self {
        Self {
            peer_id,
            addresses,
            state: DialState::PendingDial,
            retry_strategy: retry_config.strategy(),
            next_dial_time: Instant::now(),
            waker: None,
            sleeper: None,
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
            FromSwarm::DialFailure(DialFailure { peer_id: Some(peer_id), .. })
                if peer_id == self.peer_id =>
            {
                if self.state != DialState::Dialing {
                    return;
                }
                let backoff =
                    self.retry_strategy.next().expect("ExponentialBackoff is an infinite iterator");
                self.state = DialState::PendingDial;
                self.next_dial_time = Instant::now() + backoff;
                self.sleeper = None;
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
        self.sleeper = None;
        self.state = DialState::Dialing;
        debug!(?self.peer_id, addresses = ?self.addresses, "Dialling peer");
        ToSwarm::Dial {
            opts: DialOpts::peer_id(self.peer_id)
                .addresses(self.addresses.clone())
                .condition(PeerCondition::DisconnectedAndNotDialing)
                .build(),
        }
    }
}

impl Stream for DialPeerStream {
    type Item = ToSwarm<
        ToOtherBehaviourEvent,
        <dummy::ConnectionHandler as ConnectionHandler>::FromBehaviour,
    >;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.waker = Some(cx.waker().clone());

        match self.state {
            DialState::Done => Poll::Ready(None),
            DialState::Dialing => Poll::Pending,
            DialState::PendingDial => {
                let now = Instant::now();
                if self.next_dial_time <= now {
                    return Poll::Ready(Some(self.emit_dial()));
                }
                if self.sleeper.is_none() {
                    self.sleeper = Some(Box::pin(tokio::time::sleep_until(self.next_dial_time)));
                }
                let sleeper = self
                    .sleeper
                    .as_mut()
                    .expect("Sleeper cannot be None after being created above.");
                match sleeper.as_mut().poll(cx) {
                    Poll::Ready(()) => Poll::Ready(Some(self.emit_dial())),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}
