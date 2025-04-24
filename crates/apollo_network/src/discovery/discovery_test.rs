// TODO(shahak): add flow test

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use assert_matches::assert_matches;
use futures::{FutureExt, Stream, StreamExt};
use libp2p::core::{ConnectedPoint, Endpoint};
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
use tokio::time::timeout;
use void::Void;

use super::{Behaviour, DiscoveryConfig, RetryConfig, ToOtherBehaviourEvent};

const TIMEOUT: Duration = Duration::from_secs(1);
const BOOTSTRAP_DIAL_SLEEP_MILLIS: u64 = 1000; // 1 second
const BOOTSTRAP_DIAL_SLEEP: Duration = Duration::from_millis(BOOTSTRAP_DIAL_SLEEP_MILLIS);

const CONFIG: DiscoveryConfig = DiscoveryConfig {
    bootstrap_dial_retry_config: RetryConfig {
        base_delay_millis: BOOTSTRAP_DIAL_SLEEP_MILLIS,
        max_delay_seconds: BOOTSTRAP_DIAL_SLEEP,
        factor: 1,
    },
    heartbeat_interval: Duration::ZERO,
};

const CONFIG_WITH_ONE_SECOND_HEARTBEAT: DiscoveryConfig =
    DiscoveryConfig { heartbeat_interval: Duration::from_secs(1), ..CONFIG };
const CONFIG_WITH_HEARTBEAT_TWICE_BOOTSTRAP_DIAL_SLEEP: DiscoveryConfig = DiscoveryConfig {
    heartbeat_interval: Duration::from_millis(2 * BOOTSTRAP_DIAL_SLEEP_MILLIS),
    ..CONFIG
};
const CONFIG_WITH_ONE_SECOND_HEARTBEAT_AND_FIVE_SECONDS_BOOTSTRAP_DIAL_SLEEP: DiscoveryConfig =
    DiscoveryConfig {
        heartbeat_interval: Duration::from_secs(1),
        bootstrap_dial_retry_config: RetryConfig {
            base_delay_millis: 5000,
            max_delay_seconds: Duration::from_secs(5),
            factor: 1,
        },
    };
const CONFIG_WITH_LARGE_HEARTBEAT: DiscoveryConfig =
    DiscoveryConfig { heartbeat_interval: Duration::from_secs(9999999), ..CONFIG };

impl Unpin for Behaviour {}

impl Stream for Behaviour {
    type Item = ToSwarm<ToOtherBehaviourEvent, Void>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event.map_in(|e| e.left().unwrap()))),
        }
    }
}

// TODO(shahak): Make the tests resilient to the order of events.

// In case we have a bug when we return pending and then return an event.
const TIMES_TO_CHECK_FOR_PENDING_EVENT: usize = 5;

fn assert_no_event(behaviour: &mut Behaviour) {
    for _ in 0..TIMES_TO_CHECK_FOR_PENDING_EVENT {
        assert!(behaviour.next().now_or_never().is_none());
    }
}

async fn check_event_happens_after_given_duration(
    behaviour: &mut Behaviour,
    duration: Duration,
) -> ToSwarm<ToOtherBehaviourEvent, Void> {
    const EPSILON_SLEEP: Duration = Duration::from_millis(5);

    // Check that there are no events until we sleep for enough time.
    tokio::time::pause();
    tokio::time::advance(duration - EPSILON_SLEEP).await;
    assert_no_event(behaviour);

    // Sleep and check for event.
    tokio::time::advance(2 * EPSILON_SLEEP).await;
    tokio::time::resume();
    behaviour.next().now_or_never().unwrap().unwrap()
}

fn make_behaviour(
    config: &DiscoveryConfig,
    bootstrap_peer_id: &PeerId,
    bootstrap_peer_address: &Multiaddr,
) -> Behaviour {
    Behaviour::new(config.clone(), *bootstrap_peer_id, bootstrap_peer_address.clone())
}

#[rstest::fixture]
fn bootstrap_peer_id() -> PeerId {
    PeerId::random()
}

#[rstest::fixture]
fn bootstrap_peer_address() -> Multiaddr {
    Multiaddr::empty()
}

#[rstest::rstest]
#[tokio::test]
async fn discovery_redials_on_dial_failure(
    #[values(CONFIG_WITH_HEARTBEAT_TWICE_BOOTSTRAP_DIAL_SLEEP)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = make_behaviour(&config, &bootstrap_peer_id, &bootstrap_peer_address);

    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(bootstrap_peer_id)
    );

    // Consume the first query event.
    behaviour.next().await.unwrap();

    behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(bootstrap_peer_id),
        error: &DialError::Aborted,
        connection_id: ConnectionId::new_unchecked(0),
    }));

    let event =
        check_event_happens_after_given_duration(&mut behaviour, BOOTSTRAP_DIAL_SLEEP).await;
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(bootstrap_peer_id)
    );
}

#[rstest::rstest]
#[tokio::test]
async fn discovery_redials_when_all_connections_closed(
    #[values(CONFIG_WITH_LARGE_HEARTBEAT)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = make_behaviour(&config, &bootstrap_peer_id, &bootstrap_peer_address);
    connect_to_bootstrap_node(&mut behaviour, bootstrap_peer_id, bootstrap_peer_address.clone())
        .await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
        peer_id: bootstrap_peer_id,
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: bootstrap_peer_address.clone(),
            role_override: Endpoint::Dialer,
        },
        remaining_established: 0,
    }));

    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(bootstrap_peer_id)
    );
}

#[rstest::rstest]
#[tokio::test]
async fn discovery_doesnt_redial_when_one_connection_closes(
    #[values(CONFIG_WITH_LARGE_HEARTBEAT)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = make_behaviour(&config, &bootstrap_peer_id, &bootstrap_peer_address);
    connect_to_bootstrap_node(&mut behaviour, bootstrap_peer_id, bootstrap_peer_address.clone())
        .await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id: bootstrap_peer_id,
        connection_id: ConnectionId::new_unchecked(1),
        endpoint: &ConnectedPoint::Dialer {
            address: bootstrap_peer_address.clone(),
            role_override: Endpoint::Dialer,
        },
        failed_addresses: &[],
        other_established: 1,
    }));

    behaviour.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
        peer_id: bootstrap_peer_id,
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: bootstrap_peer_address.clone(),
            role_override: Endpoint::Dialer,
        },
        remaining_established: 1,
    }));

    assert_no_event(&mut behaviour);
}

async fn connect_to_bootstrap_node(
    behaviour: &mut Behaviour,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    // Consume the dial event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id: bootstrap_peer_id,
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: bootstrap_peer_address.clone(),
            role_override: Endpoint::Dialer,
        },
        failed_addresses: &[],
        other_established: 0,
    }));

    // Consume the event to add the bootstrap node to kademlia.
    // TODO(shahak): Consider extracting the validation to a separate test.
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::FoundListenAddresses {
                peer_id,
                listen_addresses,
            }
        ) if peer_id == bootstrap_peer_id && listen_addresses == vec![bootstrap_peer_address]
    );
}

#[rstest::rstest]
#[tokio::test]
async fn discovery_sleeps_between_queries(
    #[values(CONFIG_WITH_ONE_SECOND_HEARTBEAT)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = make_behaviour(&config, &bootstrap_peer_id, &bootstrap_peer_address);
    connect_to_bootstrap_node(&mut behaviour, bootstrap_peer_id, bootstrap_peer_address).await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    let event =
        check_event_happens_after_given_duration(&mut behaviour, config.heartbeat_interval).await;
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );
}

#[rstest::rstest]
#[tokio::test]
async fn discovery_performs_queries_even_if_not_connected_to_bootstrap_peer(
    #[values(CONFIG_WITH_ONE_SECOND_HEARTBEAT_AND_FIVE_SECONDS_BOOTSTRAP_DIAL_SLEEP)]
    config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = make_behaviour(&config, &bootstrap_peer_id, &bootstrap_peer_address);

    // Consume the initial dial and query events.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    // Simulate dial failure.
    behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(bootstrap_peer_id),
        error: &DialError::Aborted,
        connection_id: ConnectionId::new_unchecked(0),
    }));

    // Check that we get a new Kad query after HEARTBEAT_INTERVAL.
    let event =
        check_event_happens_after_given_duration(&mut behaviour, config.heartbeat_interval).await;
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );
}
