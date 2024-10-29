// TODO(shahak): add flow test

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use assert_matches::assert_matches;
use futures::future::pending;
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
use tokio::select;
use tokio::sync::Mutex;
use tokio::time::timeout;
use void::Void;

use super::kad_impl::KadToOtherBehaviourEvent;
use super::{Behaviour, DiscoveryConfig, RetryConfig, ToOtherBehaviourEvent};
use crate::mixed_behaviour;
use crate::mixed_behaviour::BridgedBehaviour;
use crate::test_utils::next_on_mutex_stream;

const TIMEOUT: Duration = Duration::from_secs(1);
const BOOTSTRAP_DIAL_SLEEP: Duration = Duration::from_secs(1);

const CONFIG: DiscoveryConfig = DiscoveryConfig {
    bootstrap_dial_retry_config: RetryConfig {
        base_delay_millis: BOOTSTRAP_DIAL_SLEEP.as_millis() as u64,
        max_delay: BOOTSTRAP_DIAL_SLEEP,
        factor: 1,
    },
    heartbeat_interval: Duration::ZERO,
};

impl Unpin for Behaviour {}

impl Stream for Behaviour {
    type Item = ToSwarm<ToOtherBehaviourEvent, Void>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

// In case we have a bug when we return pending and then return an event.
const TIMES_TO_CHECK_FOR_PENDING_EVENT: usize = 5;

fn assert_no_event(behaviour: &mut Behaviour) {
    for _ in 0..TIMES_TO_CHECK_FOR_PENDING_EVENT {
        assert!(behaviour.next().now_or_never().is_none());
    }
}

#[tokio::test]
async fn discovery_outputs_dial_request_on_start_without_query() {
    let bootstrap_peer_id = PeerId::random();
    let bootstrap_peer_address = Multiaddr::empty();

    let mut behaviour = Behaviour::new(CONFIG, bootstrap_peer_id, bootstrap_peer_address);

    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(bootstrap_peer_id)
    );

    assert_no_event(&mut behaviour);
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

#[tokio::test]
async fn discovery_redials_on_dial_failure() {
    let bootstrap_peer_id = PeerId::random();
    let bootstrap_peer_address = Multiaddr::empty();

    let mut behaviour = Behaviour::new(CONFIG, bootstrap_peer_id, bootstrap_peer_address);

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

    let event =
        check_event_happens_after_given_duration(&mut behaviour, BOOTSTRAP_DIAL_SLEEP).await;
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(bootstrap_peer_id)
    );
}

#[tokio::test]
async fn discovery_redials_when_all_connections_closed() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node(CONFIG).await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
        peer_id: behaviour.bootstrap_peer_id(),
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: behaviour.bootstrap_peer_address().clone(),
            role_override: Endpoint::Dialer,
        },
        remaining_established: 0,
    }));

    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(behaviour.bootstrap_peer_id())
    );
}

#[tokio::test]
async fn discovery_doesnt_redial_when_one_connection_closes() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node(CONFIG).await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id: behaviour.bootstrap_peer_id(),
        connection_id: ConnectionId::new_unchecked(1),
        endpoint: &ConnectedPoint::Dialer {
            address: behaviour.bootstrap_peer_address().clone(),
            role_override: Endpoint::Dialer,
        },
        failed_addresses: &[],
        other_established: 1,
    }));

    behaviour.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
        peer_id: behaviour.bootstrap_peer_id(),
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: behaviour.bootstrap_peer_address().clone(),
            role_override: Endpoint::Dialer,
        },
        remaining_established: 1,
    }));

    assert_no_event(&mut behaviour);
}

async fn create_behaviour_and_connect_to_bootstrap_node(config: DiscoveryConfig) -> Behaviour {
    let bootstrap_peer_id = PeerId::random();
    let bootstrap_peer_address = Multiaddr::empty();

    let mut behaviour = Behaviour::new(config, bootstrap_peer_id, bootstrap_peer_address.clone());

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

    behaviour
}

#[tokio::test]
async fn discovery_outputs_single_query_after_connecting() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node(CONFIG).await;

    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );

    assert_no_event(&mut behaviour);
}

#[tokio::test]
async fn discovery_outputs_single_query_on_query_finished() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node(CONFIG).await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_other_behaviour_event(&mixed_behaviour::ToOtherBehaviourEvent::Kad(
        KadToOtherBehaviourEvent::KadQueryFinished,
    ));
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );
}

#[tokio::test]
async fn discovery_sleeps_between_queries() {
    let mut config = CONFIG;
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
    config.heartbeat_interval = HEARTBEAT_INTERVAL;

    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node(config).await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    // Report that the query has finished
    behaviour.on_other_behaviour_event(&mixed_behaviour::ToOtherBehaviourEvent::Kad(
        KadToOtherBehaviourEvent::KadQueryFinished,
    ));

    let event = check_event_happens_after_given_duration(&mut behaviour, HEARTBEAT_INTERVAL).await;
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );
}

#[tokio::test]
async fn discovery_awakes_on_query_finished() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node(CONFIG).await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    let mutex = Mutex::new(behaviour);

    select! {
        _ = async {
            mutex.lock().await.on_other_behaviour_event(
                &mixed_behaviour::ToOtherBehaviourEvent::Kad(
                    KadToOtherBehaviourEvent::KadQueryFinished,
                )
            );
            timeout(TIMEOUT, pending::<()>()).await.unwrap();
        } => {},
        maybe_event = next_on_mutex_stream(&mutex) => assert_matches!(
            maybe_event.unwrap(),
            ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
        ),
    }
}
