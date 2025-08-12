use std::pin::Pin;
use std::task::{Context, Poll};

use futures::{FutureExt, Stream, StreamExt};
use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::multihash::Multihash;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{
    ConnectionClosed,
    ConnectionDenied,
    ConnectionId,
    DialError,
    DialFailure,
    FromSwarm,
    ListenFailure,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use rstest::rstest;

use super::OneConnectionPerPeerBehaviour;

#[derive(Debug, Clone, Copy)]
enum ConnectionType {
    Inbound,
    Outbound,
}

impl Unpin for OneConnectionPerPeerBehaviour {}

impl Stream for OneConnectionPerPeerBehaviour {
    type Item = ToSwarm<futures::never::Never, libp2p::swarm::dummy::ConnectionHandler>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_event) => {
                // OneConnectionPerPeerBehaviour never emits events, so this should never happen
                unreachable!("OneConnectionPerPeerBehaviour should never emit events")
            }
        }
    }
}

fn assert_no_event(behaviour: &mut OneConnectionPerPeerBehaviour) {
    let next_event = behaviour.next().now_or_never();
    assert!(next_event.is_none(), "Expected None, received event");
}

fn get_peer_id(peer_index: u8) -> PeerId {
    let input_digest = vec![peer_index; 32];
    PeerId::from_multihash(Multihash::wrap(0x0, &input_digest).unwrap()).unwrap()
}

/// Get a sample multiaddr for testing
fn get_multiaddr() -> Multiaddr {
    "/ip4/127.0.0.1/tcp/8080".parse().unwrap()
}

/// Create a new connection ID for testing
fn get_connection_id(id: usize) -> ConnectionId {
    ConnectionId::new_unchecked(id)
}

/// Establishes a connection of the specified type
fn establish_connection(
    behaviour: &mut OneConnectionPerPeerBehaviour,
    connection_type: ConnectionType,
    peer_id: PeerId,
    connection_id: ConnectionId,
    other_established: usize,
) -> Result<libp2p::swarm::dummy::ConnectionHandler, ConnectionDenied> {
    let addr = get_multiaddr();
    
    let result = match connection_type {
        ConnectionType::Inbound => {
            behaviour.handle_established_inbound_connection(
                connection_id,
                peer_id,
                &addr,
                &addr,
            )
        }
        ConnectionType::Outbound => {
            behaviour.handle_established_outbound_connection(
                connection_id,
                peer_id,
                &addr,
                Endpoint::Dialer,
            )
        }
    };

    // If connection was accepted, simulate the swarm event
    if result.is_ok() {
        let endpoint = match connection_type {
            ConnectionType::Inbound => ConnectedPoint::Listener {
                local_addr: addr.clone(),
                send_back_addr: addr.clone(),
            },
            ConnectionType::Outbound => ConnectedPoint::Dialer {
                address: addr.clone(),
                role_override: Endpoint::Dialer,
            },
        };

        behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint: &endpoint,
            failed_addresses: &[],
            other_established,
        }));
    }

    result
}

/// Sends connection closed event to the behaviour.
fn close_connection(
    behaviour: &mut OneConnectionPerPeerBehaviour,
    peer_id: PeerId,
    connection_id: ConnectionId,
    remaining_established: usize,
) {
    let addr = get_multiaddr();
    behaviour.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
        peer_id,
        connection_id,
        endpoint: &ConnectedPoint::Dialer { address: addr, role_override: Endpoint::Dialer },
        remaining_established,
    }));
}

/// Sends failure event to the behaviour.
fn fail_connection(
    behaviour: &mut OneConnectionPerPeerBehaviour,
    peer_id: Option<PeerId>,
    connection_id: ConnectionId,
    is_dial_failure: bool,
) {
    if is_dial_failure {
        behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
            peer_id,
            error: &DialError::Aborted,
            connection_id,
        }));
    } else {
        let local_addr = get_multiaddr();
        let mut send_back_addr = get_multiaddr();
        if let Some(peer_id) = peer_id {
            send_back_addr.push(libp2p::multiaddr::Protocol::P2p(peer_id));
        }

        behaviour.on_swarm_event(FromSwarm::ListenFailure(ListenFailure {
            local_addr: &local_addr,
            send_back_addr: &send_back_addr,
            error: &libp2p::swarm::ListenError::Aborted,
            connection_id,
        }));
    }
}

#[test]
fn test_new_behaviour_has_no_connected_peers() {
    let behaviour = OneConnectionPerPeerBehaviour::default();
    assert!(behaviour.connected_peers.is_empty());
}

#[rstest]
#[case(ConnectionType::Inbound)]
#[case(ConnectionType::Outbound)]
fn test_behaviour_accepts_first_connection(#[case] connection_type: ConnectionType) {
    let mut behaviour = OneConnectionPerPeerBehaviour::default();
    let peer_id = get_peer_id(1);
    let connection_id = get_connection_id(1);

    let result = establish_connection(&mut behaviour, connection_type, peer_id, connection_id, 0);

    result.unwrap();
    assert!(behaviour.connected_peers.contains(&peer_id));
}

#[rstest]
#[case(ConnectionType::Inbound, ConnectionType::Inbound)]
#[case(ConnectionType::Outbound, ConnectionType::Outbound)]
#[case(ConnectionType::Inbound, ConnectionType::Outbound)]
#[case(ConnectionType::Outbound, ConnectionType::Inbound)]
fn test_behaviour_denies_second_connection_to_same_peer(
    #[case] first_type: ConnectionType,
    #[case] second_type: ConnectionType,
) {
    let mut behaviour = OneConnectionPerPeerBehaviour::default();
    let peer_id = get_peer_id(1);
    let connection_id_1 = get_connection_id(1);
    let connection_id_2 = get_connection_id(2);

    // Establish first connection
    let result1 = establish_connection(&mut behaviour, first_type, peer_id, connection_id_1, 0);
    result1.unwrap();

    // Try to establish second connection to same peer
    let addr = get_multiaddr();
    let result2 = match second_type {
        ConnectionType::Inbound => behaviour.handle_established_inbound_connection(
            connection_id_2,
            peer_id,
            &addr,
            &addr,
        ),
        ConnectionType::Outbound => behaviour.handle_established_outbound_connection(
            connection_id_2,
            peer_id,
            &addr,
            Endpoint::Dialer,
        ),
    };

    assert!(result2.is_err(), "Expected connection to be denied, but it was accepted");
}

#[test]
fn test_behaviour_allows_connections_to_different_peers() {
    let mut behaviour = OneConnectionPerPeerBehaviour::default();
    let peer_id_1 = get_peer_id(1);
    let peer_id_2 = get_peer_id(2);
    let connection_id_1 = get_connection_id(1);
    let connection_id_2 = get_connection_id(2);

    // Establish connections to different peers
    let result1 = establish_connection(&mut behaviour, ConnectionType::Inbound, peer_id_1, connection_id_1, 0);
    let result2 = establish_connection(&mut behaviour, ConnectionType::Outbound, peer_id_2, connection_id_2, 0);

    result1.unwrap();
    result2.unwrap();
    assert!(behaviour.connected_peers.contains(&peer_id_1));
    assert!(behaviour.connected_peers.contains(&peer_id_2));
    assert_eq!(behaviour.connected_peers.len(), 2);
}

#[rstest]
#[case(0)] // Normal close
#[case(1)] // Edge case: remaining connections
fn test_connection_closed_removes_peer(#[case] remaining_established: usize) {
    let mut behaviour = OneConnectionPerPeerBehaviour::default();
    let peer_id = get_peer_id(1);
    let connection_id = get_connection_id(1);

    // Establish connection
    let result = establish_connection(&mut behaviour, ConnectionType::Inbound, peer_id, connection_id, 0);
    result.unwrap();
    assert!(behaviour.connected_peers.contains(&peer_id));

    // Close connection
    close_connection(&mut behaviour, peer_id, connection_id, remaining_established);

    // Peer should be removed regardless of remaining_established value
    assert!(!behaviour.connected_peers.contains(&peer_id));
}

#[test]
fn test_reconnection_after_close() {
    let mut behaviour = OneConnectionPerPeerBehaviour::default();
    let peer_id = get_peer_id(1);
    let connection_id_1 = get_connection_id(1);
    let connection_id_2 = get_connection_id(2);

    // Establish, close, then re-establish connection
    let result1 = establish_connection(&mut behaviour, ConnectionType::Inbound, peer_id, connection_id_1, 0);
    result1.unwrap();

    close_connection(&mut behaviour, peer_id, connection_id_1, 0);
    assert!(!behaviour.connected_peers.contains(&peer_id));

    let result2 = establish_connection(&mut behaviour, ConnectionType::Outbound, peer_id, connection_id_2, 0);
    result2.unwrap();
    assert!(behaviour.connected_peers.contains(&peer_id));
}

#[rstest]
#[case(true, Some(get_peer_id(1)))] // Dial failure with peer_id
#[case(true, None)] // Dial failure without peer_id
#[case(false, Some(get_peer_id(1)))] // Listen failure with peer_id
#[case(false, None)] // Listen failure without peer_id
fn test_connection_failures_remove_peer(
    #[case] is_dial_failure: bool,
    #[case] peer_id: Option<PeerId>,
) {
    let mut behaviour = OneConnectionPerPeerBehaviour::default();
    let connection_id = get_connection_id(1);

    // If we have a peer_id, establish a connection first
    if let Some(peer_id) = peer_id {
        let result = establish_connection(&mut behaviour, ConnectionType::Inbound, peer_id, connection_id, 0);
        result.unwrap();
        assert!(behaviour.connected_peers.contains(&peer_id));
    }

    // Simulate failure
    fail_connection(&mut behaviour, peer_id, connection_id, is_dial_failure);

    // If peer_id was provided, it should be removed; otherwise set should remain empty
    if let Some(peer_id) = peer_id {
        assert!(!behaviour.connected_peers.contains(&peer_id));
    }
    assert!(behaviour.connected_peers.is_empty());
}

#[rstest]
#[case(0)] // Normal case
#[case(1)] // Edge case: other_established > 0
fn test_connection_established_with_other_established(#[case] other_established: usize) {
    let mut behaviour = OneConnectionPerPeerBehaviour::default();
    let peer_id = get_peer_id(1);
    let connection_id = get_connection_id(1);

    let result = establish_connection(&mut behaviour, ConnectionType::Inbound, peer_id, connection_id, other_established);

    // Connection should be accepted regardless of other_established value
    result.unwrap();
    assert!(behaviour.connected_peers.contains(&peer_id));
}

#[test]
fn test_behaviour_invariants() {
    let mut behaviour = OneConnectionPerPeerBehaviour::default();
    let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());

    // Poll should always return pending
    assert!(matches!(behaviour.poll(&mut cx), Poll::Pending));
    
    // Behaviour should never generate events
    assert_no_event(&mut behaviour);
}

#[test]
fn test_behaviour_generates_no_events_during_lifecycle() {
    let mut behaviour = OneConnectionPerPeerBehaviour::default();
    let peer_id = get_peer_id(1);
    let connection_id = get_connection_id(1);

    // Should not generate events at any point in connection lifecycle
    assert_no_event(&mut behaviour);

    let _ = establish_connection(&mut behaviour, ConnectionType::Inbound, peer_id, connection_id, 0);
    assert_no_event(&mut behaviour);

    close_connection(&mut behaviour, peer_id, connection_id, 0);
    assert_no_event(&mut behaviour);
}
