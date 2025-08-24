// TODO(shahak): add flow test

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use assert_matches::assert_matches;
use futures::{FutureExt, Stream, StreamExt};
use libp2p::core::transport::PortUse;
use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::multihash::Multihash;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{
    ConnectionClosed,
    ConnectionId,
    DialError,
    DialFailure,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use rstest::rstest;
use tokio::time::timeout;

use super::BootstrappingBehaviour;
use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

// Timeout for waiting for events in the tests.
const TIMEOUT: Duration = Duration::from_millis(10);

// sleep seconds on dial fail should be 0.016, 0.256, 4.096, 10, 10, ...
const BOOTSTRAP_DIAL_SLEEP_BASE_MILLIS: u64 = 16;
const BOOTSTRAP_DIAL_SLEEP_BASE: Duration = Duration::from_millis(BOOTSTRAP_DIAL_SLEEP_BASE_MILLIS);
const BOOTSTRAP_DIAL_SLEEP_MAX: Duration = Duration::from_secs(10);

const CONFIG: RetryConfig = RetryConfig {
    base_delay_millis: BOOTSTRAP_DIAL_SLEEP_BASE_MILLIS,
    max_delay_seconds: BOOTSTRAP_DIAL_SLEEP_MAX,
    factor: 1,
};

const LOCAL_PEER_ID_INDEX: u8 = 0;

impl Unpin for BootstrappingBehaviour {}

impl Stream for BootstrappingBehaviour {
    type Item = ToSwarm<ToOtherBehaviourEvent, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

// In case we have a bug when we return pending and then return an event.
const TIMES_TO_CHECK_FOR_PENDING_EVENT: usize = 5;

fn assert_no_event(behaviour: &mut BootstrappingBehaviour) {
    for _ in 0..TIMES_TO_CHECK_FOR_PENDING_EVENT {
        let next_event = behaviour.next().now_or_never();
        assert!(next_event.is_none(), "Expected None, received {next_event:?}");
    }
}

async fn assert_no_event_happens_before_duration(
    behaviour: &mut BootstrappingBehaviour,
    duration: Duration,
) {
    const EPSILON_SLEEP: Duration = Duration::from_millis(5);
    // check no events now
    assert_no_event(behaviour);

    // Check that there are no events until we sleep for enough time.
    tokio::time::pause();
    tokio::time::advance(duration - EPSILON_SLEEP).await;
    assert_no_event(behaviour);

    // Sleep and check for event.
    tokio::time::advance(2 * EPSILON_SLEEP).await;
    tokio::time::resume();
}

fn get_peer_id(peer_index: u8) -> PeerId {
    let input_digest = vec![peer_index; 32];
    PeerId::from_multihash(Multihash::wrap(0x0, &input_digest).unwrap()).unwrap()
}

/// Sample a random list of peers.
fn get_peers(peer_count: usize) -> Vec<(PeerId, Multiaddr)> {
    let peer_start_index: usize = usize::from(LOCAL_PEER_ID_INDEX + 1);
    (peer_start_index..peer_start_index + peer_count)
        .map(|i| {
            let peer_index = u8::try_from(i).expect("Number of peers too high");
            (get_peer_id(peer_index), Multiaddr::empty())
        })
        .collect::<Vec<_>>()
}

/// Consumes the events that dial the bootstrap peers.
/// This function checks the dial events after construction of the behaviour.
async fn consume_dial_events(
    behaviour: &mut BootstrappingBehaviour,
    mut bootstrap_peers: Vec<(PeerId, Multiaddr)>,
) {
    let peer_count = bootstrap_peers.len();
    for _ in 0..peer_count {
        let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
        let bootstrap_peer_id = assert_matches!(
            event,
            ToSwarm::Dial{opts} => opts.get_peer_id().unwrap()
        );
        let index_to_remove = bootstrap_peers
            .iter()
            .position(|(peer_id, _)| peer_id == &bootstrap_peer_id)
            .expect("Got event for peer that is not in the list");
        bootstrap_peers.remove(index_to_remove);
    }
}

/// Consumes the events that found listen addresses.
async fn consume_found_listen_address_events(
    behaviour: &mut BootstrappingBehaviour,
    mut bootstrap_peers: Vec<(PeerId, Multiaddr)>,
) {
    let peer_count = bootstrap_peers.len();
    for _ in 0..peer_count {
        let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
        let (bootstrap_peer_id, bootstrap_addresses) = assert_matches!(
            event,
            ToSwarm::GenerateEvent( ToOtherBehaviourEvent::FoundListenAddresses {
                peer_id,
                listen_addresses
            }) => (peer_id, listen_addresses)
        );
        for address in bootstrap_addresses {
            let index_to_remove = bootstrap_peers
                .iter()
                .position(|(peer_id, peer_address)| {
                    peer_id == &bootstrap_peer_id && peer_address == &address
                })
                .expect("Got event for peer that is not in the list");
            bootstrap_peers.remove(index_to_remove);
        }
    }
}

/// Sends dial fail event to the behaviour.
fn fail_dial_attempt(behaviour: &mut BootstrappingBehaviour, peer_id: PeerId) {
    behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(peer_id),
        error: &DialError::Aborted,
        connection_id: ConnectionId::new_unchecked(0),
    }));
}

/// Sends dial fail event to the behaviour for all bootstrap peers.
fn fail_all_dial_attempts(
    behaviour: &mut BootstrappingBehaviour,
    bootstrap_peers: &[(PeerId, Multiaddr)],
) {
    for peer_id in bootstrap_peers.iter().map(|(peer_id, _)| peer_id).copied() {
        fail_dial_attempt(behaviour, peer_id);
    }
}

/// Sends connection established event to the behaviour.
fn accept_dial_attempt(
    behaviour: &mut BootstrappingBehaviour,
    peer_id: PeerId,
    other_established: usize,
) {
    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id,
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: Multiaddr::empty(),
            role_override: Endpoint::Dialer,
            port_use: PortUse::Reuse,
        },
        failed_addresses: &[],
        other_established,
    }));
}

/// Sends connection established event to the behaviour for all bootstrap peers.
fn accept_all_dial_attempts(
    behaviour: &mut BootstrappingBehaviour,
    bootstrap_peers: &[(PeerId, Multiaddr)],
    other_established: usize,
) {
    for peer_id in bootstrap_peers.iter().map(|(peer_id, _)| peer_id).copied() {
        accept_dial_attempt(behaviour, peer_id, other_established);
    }
}

/// Sends connection closed event to the behaviour.
fn close_connection(
    behaviour: &mut BootstrappingBehaviour,
    peer_id: PeerId,
    peer_address: Multiaddr,
    remaining_established: usize,
) {
    behaviour.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
        peer_id,
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: peer_address.clone(),
            role_override: Endpoint::Dialer,
            port_use: PortUse::Reuse,
        },
        remaining_established,
        cause: None,
    }));
}

/// Sends connection closed event to the behaviour for all bootstrap peers.
fn close_all_connections(
    behaviour: &mut BootstrappingBehaviour,
    bootstrap_peers: &[(PeerId, Multiaddr)],
    remaining_established: usize,
) {
    for (peer_id, address) in bootstrap_peers.iter() {
        close_connection(behaviour, *peer_id, address.clone(), remaining_established);
    }
}

// /// Checks that the behaviour dials the peer after the given duration and not before.
// async fn expect_dial_after_duration(
//     behaviour: &mut BootstrappingBehaviour,
//     peer_id: PeerId,
//     current_retry_duration: Duration,
// ) { let event = check_event_happens_after_given_duration(behaviour,
//   current_retry_duration).await;

//     assert_matches!(
//         event,
//         ToSwarm::Dial{opts} if opts.get_peer_id() == Some(peer_id)
//     );
// }

async fn make_and_connect_bootstrap_nodes(
    peer_count: usize,
) -> (Vec<(PeerId, Multiaddr)>, BootstrappingBehaviour) {
    let bootstrap_peers = get_peers(peer_count);
    let mut behaviour = BootstrappingBehaviour::new(
        get_peer_id(LOCAL_PEER_ID_INDEX),
        CONFIG,
        bootstrap_peers.clone(),
    );
    consume_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event(&mut behaviour);
    accept_all_dial_attempts(&mut behaviour, &bootstrap_peers, 0);
    consume_found_listen_address_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event(&mut behaviour);
    (bootstrap_peers, behaviour)
}

#[rstest]
#[tokio::test]
async fn bootstrapping_outputs_dial_request_per_peer_on_start(
    #[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize,
) {
    let bootstrap_peers = get_peers(peer_count);
    let mut behaviour = BootstrappingBehaviour::new(
        get_peer_id(LOCAL_PEER_ID_INDEX),
        CONFIG,
        bootstrap_peers.clone(),
    );
    consume_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event(&mut behaviour);
}

#[rstest]
#[tokio::test]
async fn bootstrapping_redials_on_dial_failure(#[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize) {
    let bootstrap_peers = get_peers(peer_count);
    let mut behaviour = BootstrappingBehaviour::new(
        get_peer_id(LOCAL_PEER_ID_INDEX),
        CONFIG,
        bootstrap_peers.clone(),
    );
    consume_dial_events(&mut behaviour, bootstrap_peers.clone()).await;

    assert_no_event(&mut behaviour);
    fail_all_dial_attempts(&mut behaviour, &bootstrap_peers);
    assert_no_event_happens_before_duration(&mut behaviour, BOOTSTRAP_DIAL_SLEEP_BASE).await;
    consume_dial_events(&mut behaviour, bootstrap_peers).await;
    assert_no_event(&mut behaviour);
}

#[rstest]
#[tokio::test]
async fn bootstrapping_redials_in_accordance_with_strategy_on_dial_failure(
    #[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize,
) {
    use std::cmp::min;
    const NUMBER_OF_DIAL_RETRIES: u32 = 10;

    let bootstrap_peers = get_peers(peer_count);
    let mut behaviour = BootstrappingBehaviour::new(
        get_peer_id(LOCAL_PEER_ID_INDEX),
        CONFIG,
        bootstrap_peers.clone(),
    );
    consume_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event(&mut behaviour);

    for i in 1..=NUMBER_OF_DIAL_RETRIES {
        fail_all_dial_attempts(&mut behaviour, &bootstrap_peers);
        let current_retry_duration = min(
            BOOTSTRAP_DIAL_SLEEP_MAX,
            Duration::from_millis(BOOTSTRAP_DIAL_SLEEP_BASE_MILLIS.pow(i)),
        );
        assert_no_event_happens_before_duration(&mut behaviour, current_retry_duration).await;
        consume_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
        assert_no_event(&mut behaviour);
    }
}

#[rstest]
#[tokio::test]
async fn bootstrapping_full_happy_flow(#[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize) {
    let (_, mut behaviour) = make_and_connect_bootstrap_nodes(peer_count).await;
    assert_no_event_happens_before_duration(&mut behaviour, BOOTSTRAP_DIAL_SLEEP_MAX * 2).await;
}

#[rstest]
#[tokio::test]
async fn bootstrapping_redials_when_all_connections_closed(
    #[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize,
) {
    let (bootstrap_peers, mut behaviour) = make_and_connect_bootstrap_nodes(peer_count).await;

    close_all_connections(&mut behaviour, &bootstrap_peers, 0);
    consume_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event_happens_before_duration(&mut behaviour, BOOTSTRAP_DIAL_SLEEP_MAX * 2).await;
}

#[rstest]
#[tokio::test]
async fn bootstrapping_redials_in_accordance_with_strategy_when_all_connections_closed(
    #[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize,
) {
    use std::cmp::min;
    const NUMBER_OF_DIAL_RETRIES: u32 = 10;

    let bootstrap_peers = get_peers(peer_count);
    let mut behaviour = BootstrappingBehaviour::new(
        get_peer_id(LOCAL_PEER_ID_INDEX),
        CONFIG,
        bootstrap_peers.clone(),
    );
    consume_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event(&mut behaviour);

    for i in 1..=NUMBER_OF_DIAL_RETRIES {
        fail_all_dial_attempts(&mut behaviour, &bootstrap_peers);
        let current_retry_duration = min(
            BOOTSTRAP_DIAL_SLEEP_MAX,
            Duration::from_millis(BOOTSTRAP_DIAL_SLEEP_BASE_MILLIS.pow(i)),
        );
        assert_no_event_happens_before_duration(&mut behaviour, current_retry_duration).await;
        consume_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
        assert_no_event(&mut behaviour);
    }

    accept_all_dial_attempts(&mut behaviour, &bootstrap_peers, 0);
    consume_found_listen_address_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event_happens_before_duration(&mut behaviour, BOOTSTRAP_DIAL_SLEEP_MAX * 2).await;

    close_all_connections(&mut behaviour, &bootstrap_peers, 0);
    consume_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event_happens_before_duration(&mut behaviour, BOOTSTRAP_DIAL_SLEEP_MAX * 2).await;

    for i in 1..=NUMBER_OF_DIAL_RETRIES {
        fail_all_dial_attempts(&mut behaviour, &bootstrap_peers);
        let current_retry_duration = min(
            BOOTSTRAP_DIAL_SLEEP_MAX,
            Duration::from_millis(BOOTSTRAP_DIAL_SLEEP_BASE_MILLIS.pow(i)),
        );
        assert_no_event_happens_before_duration(&mut behaviour, current_retry_duration).await;
        consume_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
        assert_no_event(&mut behaviour);
    }
}

#[rstest]
#[tokio::test]
async fn bootstrapping_does_not_redial_when_one_connection_closes(
    #[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize,
) {
    let (bootstrap_peers, mut behaviour) = make_and_connect_bootstrap_nodes(peer_count).await;

    assert_no_event(&mut behaviour);
    accept_all_dial_attempts(&mut behaviour, &bootstrap_peers, 0);
    assert_no_event(&mut behaviour);

    close_all_connections(&mut behaviour, &bootstrap_peers, 1);
    assert_no_event_happens_before_duration(&mut behaviour, BOOTSTRAP_DIAL_SLEEP_MAX * 2).await;
}

#[tokio::test]
async fn does_not_dial_self() {
    let local_peer_id = get_peer_id(LOCAL_PEER_ID_INDEX);
    let remote_peer_id = get_peer_id(LOCAL_PEER_ID_INDEX + 1);
    let bootstrap_peers =
        vec![(local_peer_id, Multiaddr::empty()), (remote_peer_id, Multiaddr::empty())];

    let mut behaviour = BootstrappingBehaviour::new(local_peer_id, CONFIG, bootstrap_peers.clone());
    let expected_bootstrap_peers_to_be_dialed = vec![(remote_peer_id, Multiaddr::empty())];
    consume_dial_events(&mut behaviour, expected_bootstrap_peers_to_be_dialed).await;
    assert_no_event(&mut behaviour);
}

#[tokio::test]
async fn returns_pending_if_empty_bootstrap_nodes() {
    let local_peer_id = get_peer_id(LOCAL_PEER_ID_INDEX);

    let mut behaviour = BootstrappingBehaviour::new(local_peer_id, CONFIG, vec![]);

    let mut cx = Context::from_waker(Waker::noop());
    assert_matches!(behaviour.poll(&mut cx), Poll::Pending);
}
