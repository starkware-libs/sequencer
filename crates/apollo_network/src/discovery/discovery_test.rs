// TODO(shahak): add flow test

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
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
use tokio::time::timeout;

use super::{Behaviour, DiscoveryConfig, RetryConfig, ToOtherBehaviourEvent};

const TIMEOUT: Duration = Duration::from_secs(1);
const SMALL_SLEEP_MILLIS: u64 = 1000;
const LARGE_SLEEP_MILLIS: u64 = 5000;
/// Small duration added past a deadline to ensure timers have fired.
const TIMEOUT_EPSILON: Duration = Duration::from_millis(100);

const CONFIG_WITH_ZERO_HEARTBEAT_AND_SMALL_BOOTSTRAP_DIAL: DiscoveryConfig = DiscoveryConfig {
    bootstrap_dial_retry_config: RetryConfig {
        base_delay_millis: SMALL_SLEEP_MILLIS,
        max_delay_seconds: Duration::from_millis(SMALL_SLEEP_MILLIS),
        factor: 1,
    },
    heartbeat_interval: Duration::ZERO,
};
const CONFIG_WITH_SMALL_HEARTBEAT_AND_BOOTSTRAP_SLEEP: DiscoveryConfig = DiscoveryConfig {
    heartbeat_interval: Duration::from_millis(SMALL_SLEEP_MILLIS),
    ..CONFIG_WITH_ZERO_HEARTBEAT_AND_SMALL_BOOTSTRAP_DIAL
};
const CONFIG_WITH_LARGE_HEARTBEAT_AND_SMALL_BOOTSTRAP_SLEEP: DiscoveryConfig = DiscoveryConfig {
    heartbeat_interval: Duration::from_millis(LARGE_SLEEP_MILLIS),
    ..CONFIG_WITH_ZERO_HEARTBEAT_AND_SMALL_BOOTSTRAP_DIAL
};
const CONFIG_WITH_SMALL_HEARTBEAT_AND_LARGE_BOOTSTRAP_SLEEP: DiscoveryConfig = DiscoveryConfig {
    bootstrap_dial_retry_config: RetryConfig {
        base_delay_millis: LARGE_SLEEP_MILLIS,
        max_delay_seconds: Duration::from_millis(LARGE_SLEEP_MILLIS),
        factor: 1,
    },
    ..CONFIG_WITH_SMALL_HEARTBEAT_AND_BOOTSTRAP_SLEEP
};

impl Unpin for Behaviour {}

impl Stream for Behaviour {
    type Item = ToSwarm<ToOtherBehaviourEvent, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = Pin::into_inner(self);
        loop {
            match this.poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(event) => {
                    let event = event.map_in(|_| {
                        unreachable!(
                            "The in-type is Infallible (cannot be instantiated), so the `map_in` \
                             body is unreachable."
                        )
                    });
                    // Simulate network manager routing: intercept RequestDial events
                    // and forward them to DiallingBehaviour, then re-poll.
                    if let ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestDial {
                        ref peer_id,
                        ref addresses,
                    }) = event
                    {
                        this.dialing.request_dial(*peer_id, addresses.clone());
                        continue;
                    }
                    return Poll::Ready(Some(event));
                }
            }
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
) -> ToSwarm<ToOtherBehaviourEvent, Infallible> {
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

#[rstest::fixture]
fn bootstrap_peer_id() -> PeerId {
    let dummy_digest = vec![1; 32];
    let peer_id = PeerId::from_multihash(Multihash::wrap(0x0, &dummy_digest).unwrap()).unwrap();
    assert!(peer_id != dummy_local_peer_id());
    peer_id
}

#[rstest::fixture]
fn bootstrap_peer_address() -> Multiaddr {
    Multiaddr::empty()
}

fn dummy_local_peer_id() -> PeerId {
    let dummy_digest = vec![0; 32];
    PeerId::from_multihash(Multihash::wrap(0x0, &dummy_digest).unwrap()).unwrap()
}

#[rstest::rstest]
#[tokio::test]
async fn discovery_redials_on_dial_failure(
    #[values(CONFIG_WITH_LARGE_HEARTBEAT_AND_SMALL_BOOTSTRAP_SLEEP)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = Behaviour::new(
        dummy_local_peer_id(),
        config.clone(),
        vec![(bootstrap_peer_id, bootstrap_peer_address.clone())],
    );

    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(bootstrap_peer_id)
    );

    behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(bootstrap_peer_id),
        error: &DialError::Aborted,
        connection_id: ConnectionId::new_unchecked(0),
    }));

    let event = check_event_happens_after_given_duration(
        &mut behaviour,
        config.bootstrap_dial_retry_config.max_delay_seconds,
    )
    .await;
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(bootstrap_peer_id)
    );
}

#[rstest::rstest]
#[tokio::test]
async fn discovery_redials_when_all_connections_closed(
    #[values(CONFIG_WITH_LARGE_HEARTBEAT_AND_SMALL_BOOTSTRAP_SLEEP)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = Behaviour::new(
        dummy_local_peer_id(),
        config.clone(),
        vec![(bootstrap_peer_id, bootstrap_peer_address.clone())],
    );
    connect_to_bootstrap_node(&mut behaviour, bootstrap_peer_id, bootstrap_peer_address.clone())
        .await;

    behaviour.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
        peer_id: bootstrap_peer_id,
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: bootstrap_peer_address.clone(),
            role_override: Endpoint::Dialer,
            port_use: PortUse::Reuse,
        },
        remaining_established: 0,
        cause: None,
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
    #[values(CONFIG_WITH_LARGE_HEARTBEAT_AND_SMALL_BOOTSTRAP_SLEEP)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = Behaviour::new(
        dummy_local_peer_id(),
        config.clone(),
        vec![(bootstrap_peer_id, bootstrap_peer_address.clone())],
    );
    connect_to_bootstrap_node(&mut behaviour, bootstrap_peer_id, bootstrap_peer_address.clone())
        .await;

    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id: bootstrap_peer_id,
        connection_id: ConnectionId::new_unchecked(1),
        endpoint: &ConnectedPoint::Dialer {
            address: bootstrap_peer_address.clone(),
            role_override: Endpoint::Dialer,
            port_use: PortUse::Reuse,
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
            port_use: PortUse::Reuse,
        },
        remaining_established: 1,
        cause: None,
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
            port_use: PortUse::Reuse,
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
    #[values(CONFIG_WITH_SMALL_HEARTBEAT_AND_BOOTSTRAP_SLEEP)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = Behaviour::new(
        dummy_local_peer_id(),
        config.clone(),
        vec![(bootstrap_peer_id, bootstrap_peer_address.clone())],
    );
    connect_to_bootstrap_node(&mut behaviour, bootstrap_peer_id, bootstrap_peer_address).await;

    // Set a peer to request so the heartbeat has something to query.
    behaviour.set_target_peers(std::collections::HashSet::from([PeerId::random()]));

    // The first heartbeat fires immediately since time_for_next_kad_query starts at creation time.
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );

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
    #[values(CONFIG_WITH_SMALL_HEARTBEAT_AND_LARGE_BOOTSTRAP_SLEEP)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = Behaviour::new(
        dummy_local_peer_id(),
        config.clone(),
        vec![(bootstrap_peer_id, bootstrap_peer_address.clone())],
    );

    // Consume the initial dial event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    // Simulate dial failure.
    behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(bootstrap_peer_id),
        error: &DialError::Aborted,
        connection_id: ConnectionId::new_unchecked(0),
    }));

    // Set a peer to request so the heartbeat has something to query.
    behaviour.set_target_peers(std::collections::HashSet::from([PeerId::random()]));

    // The first heartbeat fires immediately since time_for_next_kad_query starts at creation time.
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );

    // Check that we get a new Kad query after HEARTBEAT_INTERVAL.
    let event =
        check_event_happens_after_given_duration(&mut behaviour, config.heartbeat_interval).await;
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );
}

#[rstest::rstest]
#[tokio::test]
async fn set_target_peers_cancels_dials_for_removed_peers(
    #[values(CONFIG_WITH_LARGE_HEARTBEAT_AND_SMALL_BOOTSTRAP_SLEEP)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = Behaviour::new(
        dummy_local_peer_id(),
        config.clone(),
        vec![(bootstrap_peer_id, bootstrap_peer_address.clone())],
    );
    connect_to_bootstrap_node(&mut behaviour, bootstrap_peer_id, bootstrap_peer_address).await;

    let peer_a = PeerId::random();
    let peer_a_address = Multiaddr::empty();
    behaviour.set_target_peers(std::collections::HashSet::from([peer_a]));

    // Consume the KadQuery for peer_a (first heartbeat fires immediately).
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(queried_peer_id))
            if queried_peer_id == peer_a
    );

    // Simulate DHT finding addresses for peer_a.
    behaviour.kad_requesting.handle_kad_response(&[(peer_a, vec![peer_a_address])]);

    // Poll to trigger dial for peer_a.
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial { opts } if opts.get_peer_id() == Some(peer_a)
    );

    // Simulate dial failure — DialPeerStream schedules retry after some time.
    behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(peer_a),
        error: &DialError::Aborted,
        connection_id: ConnectionId::new_unchecked(0),
    }));

    // Remove peer_a from target set — this cancels its DialPeerStream.
    behaviour.set_target_peers(std::collections::HashSet::new());

    // Advance time well past the retry backoff.
    tokio::time::pause();
    tokio::time::advance(config.bootstrap_dial_retry_config.max_delay_seconds + TIMEOUT_EPSILON)
        .await;
    tokio::time::resume();

    // Assert no re-dial — the cancelled stream produces no more events.
    assert_no_event(&mut behaviour);
}

#[rstest::rstest]
#[tokio::test]
async fn set_target_peers_does_not_cancel_bootstrap_dials(
    #[values(CONFIG_WITH_LARGE_HEARTBEAT_AND_SMALL_BOOTSTRAP_SLEEP)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = Behaviour::new(
        dummy_local_peer_id(),
        config.clone(),
        vec![(bootstrap_peer_id, bootstrap_peer_address.clone())],
    );

    // Consume the initial Dial event for bootstrap peer.
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial { opts } if opts.get_peer_id() == Some(bootstrap_peer_id)
    );

    // Simulate dial failure — DialPeerStream schedules retry after some time.
    behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(bootstrap_peer_id),
        error: &DialError::Aborted,
        connection_id: ConnectionId::new_unchecked(0),
    }));

    // Set empty target peers — bootstrap peer was never in the target set, so its dial persists.
    behaviour.set_target_peers(std::collections::HashSet::new());

    // Verify the bootstrap peer is still re-dialed after backoff.
    let event = check_event_happens_after_given_duration(
        &mut behaviour,
        config.bootstrap_dial_retry_config.max_delay_seconds,
    )
    .await;
    assert_matches!(
        event,
        ToSwarm::Dial { opts } if opts.get_peer_id() == Some(bootstrap_peer_id)
    );
}

// TODO(AndrewL): cancel_dial is origin-agnostic — it cancels ALL DialPeerStreams for a peer,
// including bootstrap-originated ones. If a bootstrap peer overlaps with the target set and is
// then removed from targets, its bootstrap dial is killed too. BootstrappingBehaviour only
// re-emits RequestDial on ConnectionClosed (not on cancellation), so the peer becomes
// permanently unreachable. This test documents the current (buggy) behavior.
#[rstest::rstest]
#[tokio::test]
async fn set_target_peers_cancels_bootstrap_dial_when_bootstrap_peer_overlaps_with_targets(
    #[values(CONFIG_WITH_LARGE_HEARTBEAT_AND_SMALL_BOOTSTRAP_SLEEP)] config: DiscoveryConfig,
    bootstrap_peer_id: PeerId,
    bootstrap_peer_address: Multiaddr,
) {
    let mut behaviour = Behaviour::new(
        dummy_local_peer_id(),
        config.clone(),
        vec![(bootstrap_peer_id, bootstrap_peer_address.clone())],
    );

    // Consume the initial Dial event for bootstrap peer.
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial { opts } if opts.get_peer_id() == Some(bootstrap_peer_id)
    );

    // Simulate dial failure — DialPeerStream schedules retry after some time.
    behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(bootstrap_peer_id),
        error: &DialError::Aborted,
        connection_id: ConnectionId::new_unchecked(0),
    }));

    // Add bootstrap peer to the target set, then remove it.
    behaviour.set_target_peers(std::collections::HashSet::from([bootstrap_peer_id]));
    behaviour.set_target_peers(std::collections::HashSet::new());

    // Advance time well past the retry backoff.
    tokio::time::pause();
    tokio::time::advance(config.bootstrap_dial_retry_config.max_delay_seconds + TIMEOUT_EPSILON)
        .await;
    tokio::time::resume();

    // The bootstrap dial was cancelled — no re-dial occurs.
    assert_no_event(&mut behaviour);
}
