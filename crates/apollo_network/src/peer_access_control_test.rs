use std::collections::HashSet;
use std::task::{Context, Poll};

use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{ConnectionId, FromSwarm, NetworkBehaviour, ToSwarm};
use libp2p::{Multiaddr, PeerId};

use super::peer_access_control::Behaviour;

#[test]
fn allows_all_before_enforcement() {
    let mut behaviour = Behaviour::new();

    let random_peer = PeerId::random();
    let result = behaviour.handle_established_inbound_connection(
        ConnectionId::new_unchecked(0),
        random_peer,
        &Multiaddr::empty(),
        &Multiaddr::empty(),
    );
    assert!(result.is_ok());
}

#[test]
fn denies_non_allowed_peer() {
    let allowed_peer = PeerId::random();
    let mut behaviour = Behaviour::new();

    behaviour.set_allowed_peers(HashSet::from([allowed_peer]));

    let stranger = PeerId::random();
    let result = behaviour.handle_established_inbound_connection(
        ConnectionId::new_unchecked(0),
        stranger,
        &Multiaddr::empty(),
        &Multiaddr::empty(),
    );
    assert!(result.is_err());
}

#[test]
fn allows_allowed_peers() {
    let allowed_peer = PeerId::random();
    let mut behaviour = Behaviour::new();

    behaviour.set_allowed_peers(HashSet::from([allowed_peer]));

    let result = behaviour.handle_established_inbound_connection(
        ConnectionId::new_unchecked(0),
        allowed_peer,
        &Multiaddr::empty(),
        &Multiaddr::empty(),
    );
    assert!(result.is_ok());
}

#[test]
fn disconnects_removed_peers_on_allowed_change() {
    let peer_a = PeerId::random();
    let peer_b = PeerId::random();
    let mut behaviour = Behaviour::new();

    behaviour.set_allowed_peers(HashSet::from([peer_a, peer_b]));

    // Remove peer_b from the allowed set.
    behaviour.set_allowed_peers(HashSet::from([peer_a]));

    let disconnected = drain_close_connection_events(&mut behaviour);
    assert_eq!(disconnected, vec![peer_b]);
}

#[test]
fn denies_outbound_to_non_allowed_peer() {
    let mut behaviour = Behaviour::new();
    behaviour.set_allowed_peers(HashSet::from([PeerId::random()]));

    let stranger = PeerId::random();
    let result = behaviour.handle_pending_outbound_connection(
        ConnectionId::new_unchecked(0),
        Some(stranger),
        &[],
        Endpoint::Dialer,
    );
    assert!(result.is_err());
}

#[test]
fn allows_outbound_to_allowed_peer() {
    let allowed_peer = PeerId::random();
    let mut behaviour = Behaviour::new();
    behaviour.set_allowed_peers(HashSet::from([allowed_peer]));

    let result = behaviour.handle_pending_outbound_connection(
        ConnectionId::new_unchecked(0),
        Some(allowed_peer),
        &[],
        Endpoint::Dialer,
    );
    assert!(result.is_ok());
}

#[test]
fn empty_allowed_set_disconnects_all_previously_allowed() {
    let peer_a = PeerId::random();
    let peer_b = PeerId::random();
    let mut behaviour = Behaviour::new();

    behaviour.set_allowed_peers(HashSet::from([peer_a, peer_b]));

    behaviour.set_allowed_peers(HashSet::new());

    let disconnected = drain_close_connection_events(&mut behaviour);
    assert!(disconnected.contains(&peer_a));
    assert!(disconnected.contains(&peer_b));
}

// TODO(AndrewL): unite these test helpers with sqmr tests into a shared test util file.

fn expect_close_connection_event(behaviour: &mut Behaviour) -> Option<PeerId> {
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    match behaviour.poll(&mut cx) {
        Poll::Ready(ToSwarm::CloseConnection { peer_id, .. }) => Some(peer_id),
        Poll::Ready(other) => panic!("Unexpected event: {other:?}"),
        Poll::Pending => None,
    }
}

/// Drains all pending `CloseConnection` events, panicking if any other event type is emitted.
fn drain_close_connection_events(behaviour: &mut Behaviour) -> Vec<PeerId> {
    let mut disconnected = Vec::new();
    while let Some(peer_id) = expect_close_connection_event(behaviour) {
        disconnected.push(peer_id);
    }
    disconnected
}

#[allow(dead_code)]
fn simulate_connection_established(
    behaviour: &mut Behaviour,
    peer_id: PeerId,
    connection_id: usize,
) {
    let address = Multiaddr::empty();
    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id,
        connection_id: ConnectionId::new_unchecked(connection_id),
        endpoint: &ConnectedPoint::Listener {
            local_addr: address.clone(),
            send_back_addr: address,
        },
        failed_addresses: &[],
        other_established: 0,
    }));
}
