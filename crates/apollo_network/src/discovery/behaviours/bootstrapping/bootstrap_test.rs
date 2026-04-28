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
use libp2p::swarm::{ConnectionClosed, ConnectionId, FromSwarm, NetworkBehaviour, ToSwarm};
use libp2p::{Multiaddr, PeerId};
use rstest::rstest;
use tokio::time::timeout;

use super::BootstrappingBehaviour;
use crate::discovery::ToOtherBehaviourEvent;

const TIMEOUT: Duration = Duration::from_millis(10);
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

const TIMES_TO_CHECK_FOR_PENDING_EVENT: usize = 5;

fn assert_no_event(behaviour: &mut BootstrappingBehaviour) {
    for _ in 0..TIMES_TO_CHECK_FOR_PENDING_EVENT {
        let next_event = behaviour.next().now_or_never();
        assert!(next_event.is_none(), "Expected None, received {next_event:?}");
    }
}

fn get_peer_id(peer_index: u8) -> PeerId {
    let input_digest = vec![peer_index; 32];
    PeerId::from_multihash(Multihash::wrap(0x0, &input_digest).unwrap()).unwrap()
}

fn get_peers(peer_count: usize) -> Vec<(PeerId, Multiaddr)> {
    let peer_start_index: usize = usize::from(LOCAL_PEER_ID_INDEX + 1);
    (peer_start_index..peer_start_index + peer_count)
        .map(|i| {
            let peer_index = u8::try_from(i).expect("Number of peers too high");
            (get_peer_id(peer_index), Multiaddr::empty())
        })
        .collect::<Vec<_>>()
}

/// Consumes RequestDial events emitted for the bootstrap peers.
async fn consume_request_dial_events(
    behaviour: &mut BootstrappingBehaviour,
    mut bootstrap_peers: Vec<(PeerId, Multiaddr)>,
) {
    let peer_count = bootstrap_peers.len();
    for _ in 0..peer_count {
        let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
        let (peer_id, addresses) = assert_matches!(
            event,
            ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestDial {
                peer_id,
                addresses,
            }) => (peer_id, addresses)
        );
        let index_to_remove = bootstrap_peers
            .iter()
            .position(|(expected_id, expected_addr)| {
                *expected_id == peer_id && addresses.contains(expected_addr)
            })
            .expect("Got RequestDial for peer that is not in the list");
        bootstrap_peers.remove(index_to_remove);
    }
}

/// Consumes FoundListenAddresses events.
async fn consume_found_listen_address_events(
    behaviour: &mut BootstrappingBehaviour,
    mut bootstrap_peers: Vec<(PeerId, Multiaddr)>,
) {
    let peer_count = bootstrap_peers.len();
    for _ in 0..peer_count {
        let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
        let (bootstrap_peer_id, bootstrap_addresses) = assert_matches!(
            event,
            ToSwarm::GenerateEvent(ToOtherBehaviourEvent::FoundListenAddresses {
                peer_id,
                listen_addresses,
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

fn accept_all_dial_attempts(
    behaviour: &mut BootstrappingBehaviour,
    bootstrap_peers: &[(PeerId, Multiaddr)],
    other_established: usize,
) {
    for peer_id in bootstrap_peers.iter().map(|(peer_id, _)| peer_id).copied() {
        accept_dial_attempt(behaviour, peer_id, other_established);
    }
}

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

fn close_all_connections(
    behaviour: &mut BootstrappingBehaviour,
    bootstrap_peers: &[(PeerId, Multiaddr)],
    remaining_established: usize,
) {
    for (peer_id, address) in bootstrap_peers.iter() {
        close_connection(behaviour, *peer_id, address.clone(), remaining_established);
    }
}

async fn make_and_connect_bootstrap_nodes(
    peer_count: usize,
) -> (Vec<(PeerId, Multiaddr)>, BootstrappingBehaviour) {
    let bootstrap_peers = get_peers(peer_count);
    let mut behaviour =
        BootstrappingBehaviour::new(get_peer_id(LOCAL_PEER_ID_INDEX), bootstrap_peers.clone());
    consume_request_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event(&mut behaviour);
    accept_all_dial_attempts(&mut behaviour, &bootstrap_peers, 0);
    consume_found_listen_address_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event(&mut behaviour);
    (bootstrap_peers, behaviour)
}

#[rstest]
#[tokio::test]
async fn bootstrapping_outputs_request_dial_per_peer_on_start(
    #[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize,
) {
    let bootstrap_peers = get_peers(peer_count);
    let mut behaviour =
        BootstrappingBehaviour::new(get_peer_id(LOCAL_PEER_ID_INDEX), bootstrap_peers.clone());
    consume_request_dial_events(&mut behaviour, bootstrap_peers).await;
    assert_no_event(&mut behaviour);
}

#[rstest]
#[tokio::test]
async fn bootstrapping_full_happy_flow(#[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize) {
    let (_, mut behaviour) = make_and_connect_bootstrap_nodes(peer_count).await;
    assert_no_event(&mut behaviour);
}

#[rstest]
#[tokio::test]
async fn bootstrapping_redials_when_all_connections_closed(
    #[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize,
) {
    let (bootstrap_peers, mut behaviour) = make_and_connect_bootstrap_nodes(peer_count).await;

    close_all_connections(&mut behaviour, &bootstrap_peers, 0);
    consume_request_dial_events(&mut behaviour, bootstrap_peers).await;
    assert_no_event(&mut behaviour);
}

#[rstest]
#[tokio::test]
async fn bootstrapping_does_not_redial_when_one_connection_closes(
    #[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize,
) {
    let (bootstrap_peers, mut behaviour) = make_and_connect_bootstrap_nodes(peer_count).await;

    close_all_connections(&mut behaviour, &bootstrap_peers, 1);
    assert_no_event(&mut behaviour);
}

#[tokio::test]
async fn does_not_dial_self() {
    let local_peer_id = get_peer_id(LOCAL_PEER_ID_INDEX);
    let remote_peer_id = get_peer_id(LOCAL_PEER_ID_INDEX + 1);
    let bootstrap_peers =
        vec![(local_peer_id, Multiaddr::empty()), (remote_peer_id, Multiaddr::empty())];

    let mut behaviour = BootstrappingBehaviour::new(local_peer_id, bootstrap_peers);
    let expected_peers = vec![(remote_peer_id, Multiaddr::empty())];
    consume_request_dial_events(&mut behaviour, expected_peers).await;
    assert_no_event(&mut behaviour);
}

#[rstest]
#[tokio::test]
async fn rapid_reconnect_does_not_emit_stale_request_dial(
    #[values(1, 2, 3, 4, 5, 6, 7)] peer_count: usize,
) {
    let (bootstrap_peers, mut behaviour) = make_and_connect_bootstrap_nodes(peer_count).await;

    // Simulate rapid disconnect+reconnect in the same poll cycle:
    // ConnectionClosed queues a RequestDial, then ConnectionEstablished should cancel it.
    for (peer_id, address) in &bootstrap_peers {
        close_connection(&mut behaviour, *peer_id, address.clone(), 0);
        accept_dial_attempt(&mut behaviour, *peer_id, 0);
    }

    // Only FoundListenAddresses should remain — no stale RequestDial events.
    consume_found_listen_address_events(&mut behaviour, bootstrap_peers).await;
    assert_no_event(&mut behaviour);
}

/// Demonstrates that BootstrappingBehaviour emits RequestDial on every ConnectionClosed,
/// regardless of connection duration. The backoff protection against tight retry loops lives
/// in DialingBehaviour (which applies reconnection backoff for short-lived connections),
/// not here.
#[tokio::test]
async fn bootstrapping_emits_request_dial_on_every_connection_close() {
    const NUM_CYCLES: usize = 5;

    let bootstrap_peers = get_peers(1);
    let (peer_id, peer_address) = bootstrap_peers[0].clone();
    let mut behaviour =
        BootstrappingBehaviour::new(get_peer_id(LOCAL_PEER_ID_INDEX), bootstrap_peers.clone());

    // Consume the initial RequestDial.
    consume_request_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
    assert_no_event(&mut behaviour);

    for _ in 0..NUM_CYCLES {
        // ConnectionEstablished → emits FoundListenAddresses
        accept_dial_attempt(&mut behaviour, peer_id, 0);
        consume_found_listen_address_events(&mut behaviour, bootstrap_peers.clone()).await;

        // ConnectionClosed → emits RequestDial (always, no backoff here)
        close_connection(&mut behaviour, peer_id, peer_address.clone(), 0);
        consume_request_dial_events(&mut behaviour, bootstrap_peers.clone()).await;
        assert_no_event(&mut behaviour);
    }
}

#[tokio::test]
async fn returns_pending_if_empty_bootstrap_nodes() {
    let local_peer_id = get_peer_id(LOCAL_PEER_ID_INDEX);

    let mut behaviour = BootstrappingBehaviour::new(local_peer_id, vec![]);

    let mut cx = Context::from_waker(Waker::noop());
    assert_matches!(behaviour.poll(&mut cx), Poll::Pending);
}
