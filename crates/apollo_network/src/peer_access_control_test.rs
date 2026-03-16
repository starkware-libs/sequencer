use std::collections::HashSet;
use std::task::{Context, Poll};

use libp2p::core::transport::PortUse;
use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{ConnectionId, FromSwarm, NetworkBehaviour, ToSwarm};
use libp2p::{Multiaddr, PeerId};

use super::peer_access_control::Behaviour;

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

fn poll_close_connection(behaviour: &mut Behaviour) -> Option<PeerId> {
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    match behaviour.poll(&mut cx) {
        Poll::Ready(ToSwarm::CloseConnection { peer_id, .. }) => Some(peer_id),
        Poll::Ready(other) => panic!("Unexpected event: {other:?}"),
        Poll::Pending => None,
    }
}

fn collect_all_close_connections(behaviour: &mut Behaviour) -> Vec<PeerId> {
    let mut disconnected = Vec::new();
    while let Some(peer_id) = poll_close_connection(behaviour) {
        disconnected.push(peer_id);
    }
    disconnected
}

#[test]
fn allows_all_before_enforcement() {
    let bootstrap_peer_id = PeerId::random();
    let mut behaviour = Behaviour::new(HashSet::from([bootstrap_peer_id]));

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
fn denies_non_target_after_enforcement() {
    let bootstrap_peer_id = PeerId::random();
    let target_peer = PeerId::random();
    let mut behaviour = Behaviour::new(HashSet::from([bootstrap_peer_id]));

    behaviour.set_target_peers(HashSet::from([target_peer]));

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
fn allows_target_peers() {
    let target_peer = PeerId::random();
    let mut behaviour = Behaviour::new(HashSet::new());

    behaviour.set_target_peers(HashSet::from([target_peer]));

    let result = behaviour.handle_established_inbound_connection(
        ConnectionId::new_unchecked(0),
        target_peer,
        &Multiaddr::empty(),
        &Multiaddr::empty(),
    );
    assert!(result.is_ok());
}

#[test]
fn allows_bootstrap_peers_always() {
    let bootstrap_peer_id = PeerId::random();
    let target_peer = PeerId::random();
    let mut behaviour = Behaviour::new(HashSet::from([bootstrap_peer_id]));

    behaviour.set_target_peers(HashSet::from([target_peer]));

    let result = behaviour.handle_established_inbound_connection(
        ConnectionId::new_unchecked(0),
        bootstrap_peer_id,
        &Multiaddr::empty(),
        &Multiaddr::empty(),
    );
    assert!(result.is_ok());
}

#[test]
fn disconnects_removed_peers_on_target_change() {
    let target_peer_a = PeerId::random();
    let target_peer_b = PeerId::random();
    let mut behaviour = Behaviour::new(HashSet::new());

    behaviour.set_target_peers(HashSet::from([target_peer_a, target_peer_b]));
    simulate_connection_established(&mut behaviour, target_peer_a, 0);
    simulate_connection_established(&mut behaviour, target_peer_b, 1);

    // Remove target_peer_b from the allowed set.
    behaviour.set_target_peers(HashSet::from([target_peer_a]));

    let disconnected = collect_all_close_connections(&mut behaviour);
    assert_eq!(disconnected, vec![target_peer_b]);
}

#[test]
fn does_not_disconnect_bootstrap_peers() {
    let bootstrap_peer_id = PeerId::random();
    let target_peer = PeerId::random();
    let mut behaviour = Behaviour::new(HashSet::from([bootstrap_peer_id]));

    behaviour.set_target_peers(HashSet::from([target_peer]));
    simulate_connection_established(&mut behaviour, bootstrap_peer_id, 0);
    simulate_connection_established(&mut behaviour, target_peer, 1);

    // Set targets to empty — bootstrap peer should survive.
    behaviour.set_target_peers(HashSet::new());

    let disconnected = collect_all_close_connections(&mut behaviour);
    assert!(!disconnected.contains(&bootstrap_peer_id));
    assert!(disconnected.contains(&target_peer));
}

#[test]
fn allows_all_outbound() {
    let mut behaviour = Behaviour::new(HashSet::new());
    behaviour.set_target_peers(HashSet::from([PeerId::random()]));

    let stranger = PeerId::random();
    let result = behaviour.handle_established_outbound_connection(
        ConnectionId::new_unchecked(0),
        stranger,
        &Multiaddr::empty(),
        Endpoint::Dialer,
        PortUse::Reuse,
    );
    assert!(result.is_ok());
}

#[test]
fn empty_target_set_disconnects_all_non_bootstrap() {
    let bootstrap_peer_id = PeerId::random();
    let peer_a = PeerId::random();
    let peer_b = PeerId::random();
    let mut behaviour = Behaviour::new(HashSet::from([bootstrap_peer_id]));

    behaviour.set_target_peers(HashSet::from([peer_a, peer_b]));
    simulate_connection_established(&mut behaviour, bootstrap_peer_id, 0);
    simulate_connection_established(&mut behaviour, peer_a, 1);
    simulate_connection_established(&mut behaviour, peer_b, 2);

    behaviour.set_target_peers(HashSet::new());

    let disconnected = collect_all_close_connections(&mut behaviour);
    assert!(disconnected.contains(&peer_a));
    assert!(disconnected.contains(&peer_b));
    assert!(!disconnected.contains(&bootstrap_peer_id));
}
