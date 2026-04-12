use std::pin::Pin;
use std::time::Duration;

use futures::{FutureExt, StreamExt};
use libp2p::core::transport::PortUse;
use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::multihash::Multihash;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::PeerCondition;
use libp2p::swarm::{ConnectionId, DialError, DialFailure, FromSwarm, ToSwarm};
use libp2p::{Multiaddr, PeerId};
use rstest::{fixture, rstest};

use super::DialPeerStream;
use crate::discovery::RetryConfig;

const TEST_COOLDOWN: Duration = Duration::from_millis(200);
/// `ExponentialBackoff::from_millis(BASE).factor(FACTOR)` yields first backoff = BASE * FACTOR.
const BASE_DELAY_MILLIS: u64 = 10;
const FACTOR: u64 = 2;
const FIRST_BACKOFF: Duration = Duration::from_millis(BASE_DELAY_MILLIS * FACTOR);
const TIMES_TO_CHECK_FOR_PENDING_EVENT: usize = 5;

/// When `request_dial` is called for a peer that is already connected (but the stream was created
/// after the connection), the swarm rejects the dial with `DialPeerConditionFalse`. The stream
/// should transition to `CooldownBeforeDeletion` and terminate after cooldown — not retry forever.
#[rstest]
#[tokio::test(start_paused = true)]
async fn dial_on_already_connected_peer_enters_cooldown_and_terminates(
    retry_config: RetryConfig,
    peer_id: PeerId,
    addresses: Vec<Multiaddr>,
) {
    let mut stream = DialPeerStream::new(&retry_config, peer_id, addresses);

    let connection_id = poll_dial(&mut stream);

    // Swarm rejects the dial because the peer is already connected.
    send_dial_failure_condition_false(&mut stream, peer_id, connection_id);

    // Stream should be in CooldownBeforeDeletion, not retrying.
    assert_stream_pending(&mut stream);

    // After cooldown expires, stream terminates.
    tokio::time::advance(TEST_COOLDOWN + Duration::from_millis(1)).await;
    assert_stream_terminated(&mut stream);
}

/// When the stream dials and an external entity establishes the connection (e.g., inbound
/// connection or another behaviour's dial succeeds), the stream receives `ConnectionEstablished`
/// for a different connection id, transitions to cooldown, and terminates once the cooldown
/// window expires.
#[rstest]
#[tokio::test(start_paused = true)]
async fn external_connection_established_enters_cooldown_and_terminates(
    retry_config: RetryConfig,
    peer_id: PeerId,
    addresses: Vec<Multiaddr>,
) {
    let mut stream = DialPeerStream::new(&retry_config, peer_id, addresses);

    let dial_connection_id = poll_dial(&mut stream);

    // Another entity established a different connection to this peer.
    let other_connection_id =
        ConnectionId::new_unchecked(format!("{dial_connection_id}").parse::<usize>().unwrap() + 1);
    send_connection_established(&mut stream, peer_id, other_connection_id);

    // Stream is in CooldownBeforeDeletion — pending immediately after the event and just before
    // the cooldown window expires.
    assert_stream_pending(&mut stream);
    tokio::time::advance(TEST_COOLDOWN - Duration::from_millis(1)).await;
    assert_stream_pending(&mut stream);

    // After cooldown expires, stream terminates.
    tokio::time::advance(Duration::from_millis(2)).await;
    assert_stream_terminated(&mut stream);
}

/// When a connection was established and the stream is in cooldown, calling `request_redial`
/// transitions back to `PendingDial` with accumulated backoff. The next dial uses the new
/// addresses.
#[rstest]
#[tokio::test(start_paused = true)]
async fn request_redial_during_cooldown_reuses_accumulated_backoff(
    retry_config: RetryConfig,
    peer_id: PeerId,
    addresses: Vec<Multiaddr>,
) {
    let mut stream = DialPeerStream::new(&retry_config, peer_id, addresses);

    // First dial attempt → fail → retry (consumes first backoff slot = BASE * FACTOR = 20ms).
    let connection_id = poll_dial(&mut stream);
    send_dial_failure_aborted(&mut stream, peer_id, connection_id);

    // Wait for first retry backoff.
    tokio::time::advance(FIRST_BACKOFF + Duration::from_millis(1)).await;
    let dial_connection_id = poll_dial(&mut stream);

    // Second dial succeeds → cooldown.
    send_connection_established(&mut stream, peer_id, dial_connection_id);
    assert_stream_pending(&mut stream);

    // Request redial during cooldown with new addresses.
    let new_addresses = vec!["/ip4/1.2.3.4/tcp/1234".parse().unwrap()];
    stream.request_redial(new_addresses.clone());

    // Second backoff = current(BASE^2) * FACTOR. ExponentialBackoff grows current by BASE each
    // step: current starts at BASE(10), after first next() becomes BASE*BASE(100).
    // Second backoff = 100 * 2 = 200ms.
    let second_backoff = Duration::from_millis(BASE_DELAY_MILLIS.pow(2) * FACTOR);

    // Advancing by the first backoff duration should NOT produce a dial yet.
    tokio::time::advance(FIRST_BACKOFF + Duration::from_millis(1)).await;
    assert_stream_pending(&mut stream);

    // Advancing past the accumulated (second) backoff should produce a dial.
    tokio::time::advance(second_backoff).await;
    let _connection_id = poll_dial(&mut stream);
}

#[fixture]
fn retry_config() -> RetryConfig {
    RetryConfig {
        base_delay_millis: BASE_DELAY_MILLIS,
        max_delay_seconds: Duration::from_secs(10),
        factor: FACTOR,
        new_connection_stabilization_millis: TEST_COOLDOWN,
    }
}

#[fixture]
fn peer_id() -> PeerId {
    let digest = vec![1u8; 32];
    PeerId::from_multihash(Multihash::wrap(0x0, &digest).unwrap()).unwrap()
}

#[fixture]
fn addresses() -> Vec<Multiaddr> {
    vec![Multiaddr::empty()]
}

/// Polls the stream and asserts a `ToSwarm::Dial` event is emitted. Returns the connection id.
fn poll_dial(stream: &mut DialPeerStream) -> ConnectionId {
    let event = Pin::new(stream)
        .next()
        .now_or_never()
        .expect("Expected stream to be ready with a Dial event")
        .unwrap();
    let ToSwarm::Dial { opts } = event else { panic!("Expected Dial event, got {event:?}") };
    opts.connection_id()
}

/// Asserts the stream has no event ready (polls multiple times to drain pending wakeups).
fn assert_stream_pending(stream: &mut DialPeerStream) {
    for _ in 0..TIMES_TO_CHECK_FOR_PENDING_EVENT {
        assert!(
            Pin::new(&mut *stream).next().now_or_never().is_none(),
            "Expected stream to be pending"
        );
    }
}

/// Asserts the stream terminates (returns `Ready(None)`).
fn assert_stream_terminated(stream: &mut DialPeerStream) {
    let result = Pin::new(stream).next().now_or_never();
    assert!(
        matches!(result, Some(None)),
        "Expected stream to terminate (Ready(None)), got {result:?}"
    );
}

fn send_connection_established(
    stream: &mut DialPeerStream,
    peer_id: PeerId,
    connection_id: ConnectionId,
) {
    stream.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id,
        connection_id,
        endpoint: &ConnectedPoint::Dialer {
            address: Multiaddr::empty(),
            role_override: Endpoint::Dialer,
            port_use: PortUse::Reuse,
        },
        failed_addresses: &[],
        other_established: 0,
    }));
}

fn send_dial_failure_condition_false(
    stream: &mut DialPeerStream,
    peer_id: PeerId,
    connection_id: ConnectionId,
) {
    stream.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(peer_id),
        error: &DialError::DialPeerConditionFalse(PeerCondition::Disconnected),
        connection_id,
    }));
}

fn send_dial_failure_aborted(
    stream: &mut DialPeerStream,
    peer_id: PeerId,
    connection_id: ConnectionId,
) {
    stream.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(peer_id),
        error: &DialError::Aborted,
        connection_id,
    }));
}
