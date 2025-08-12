//! Tests for OneConnectionPerPeerBehaviour
//!
//! This module contains comprehensive tests for the OneConnectionPerPeerBehaviour,
//! which ensures that only one connection per peer is maintained at any time.
//!
//! ## Test Organization
//!
//! - **Helper Functions**: Utilities for creating test data and simulating events
//! - **Basic Functionality**: Core connection acceptance/denial behavior
//! - **Connection Lifecycle**: Connection establishment, closure, and failure scenarios
//! - **Edge Cases**: Complex scenarios like rapid-fire connections and timing edge cases
//! - **Invariants**: Behavior guarantees and invariant validation

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

// =============================================================================
// HELPER TYPES AND IMPLEMENTATIONS
// =============================================================================

#[derive(Debug, Clone, Copy)]
enum ConnectionType {
    Inbound,
    Outbound,
}

#[derive(Debug, Clone, Copy)]
enum FailureType {
    Dial,
    Listen,
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

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

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

/// Attempts to establish a connection of the specified type (without simulating swarm event)
fn attempt_connection(
    behaviour: &mut OneConnectionPerPeerBehaviour,
    connection_type: ConnectionType,
    peer_id: PeerId,
    connection_id: ConnectionId,
) -> Result<libp2p::swarm::dummy::ConnectionHandler, ConnectionDenied> {
    let addr = get_multiaddr();

    match connection_type {
        ConnectionType::Inbound => {
            behaviour.handle_established_inbound_connection(connection_id, peer_id, &addr, &addr)
        }
        ConnectionType::Outbound => behaviour.handle_established_outbound_connection(
            connection_id,
            peer_id,
            &addr,
            Endpoint::Dialer,
        ),
    }
}

/// Simulates the ConnectionEstablished swarm event
fn simulate_connection_established(
    behaviour: &mut OneConnectionPerPeerBehaviour,
    connection_type: ConnectionType,
    peer_id: PeerId,
    connection_id: ConnectionId,
    other_established: usize,
) {
    let addr = get_multiaddr();
    let endpoint = match connection_type {
        ConnectionType::Inbound => {
            ConnectedPoint::Listener { local_addr: addr.clone(), send_back_addr: addr.clone() }
        }
        ConnectionType::Outbound => {
            ConnectedPoint::Dialer { address: addr.clone(), role_override: Endpoint::Dialer }
        }
    };

    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id,
        connection_id,
        endpoint: &endpoint,
        failed_addresses: &[],
        other_established,
    }));
}

/// Helper for tests that need the old behavior (attempt + simulate if successful)
fn establish_connection(
    behaviour: &mut OneConnectionPerPeerBehaviour,
    connection_type: ConnectionType,
    peer_id: PeerId,
    connection_id: ConnectionId,
    other_established: usize,
) -> Result<libp2p::swarm::dummy::ConnectionHandler, ConnectionDenied> {
    let result = attempt_connection(behaviour, connection_type, peer_id, connection_id);
    // Only simulate the swarm event if connection was accepted
    if result.is_ok() {
        simulate_connection_established(
            behaviour,
            connection_type,
            peer_id,
            connection_id,
            other_established,
        );
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
    failure_type: FailureType,
) {
    match failure_type {
        FailureType::Dial => {
            behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
                peer_id,
                error: &DialError::Aborted,
                connection_id,
            }));
        }
        FailureType::Listen => {
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
}

// =============================================================================
// BASIC FUNCTIONALITY TESTS
// =============================================================================

/// Tests basic behavior initialization and state

mod basic_functionality {
    use super::*;

    #[test]
    fn new_behaviour_has_no_connected_peers() {
        let behaviour = OneConnectionPerPeerBehaviour::default();
        assert!(behaviour.connected_peers().is_empty());
    }

    #[rstest]
    #[case(ConnectionType::Inbound)]
    #[case(ConnectionType::Outbound)]
    fn accepts_first_connection(#[case] connection_type: ConnectionType) {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);
        let connection_id = get_connection_id(1);

        let result =
            establish_connection(&mut behaviour, connection_type, peer_id, connection_id, 0);

        result.unwrap();
        assert!(behaviour.connected_peers().contains(&peer_id));
    }

    #[rstest]
    #[case(ConnectionType::Inbound, ConnectionType::Inbound)]
    #[case(ConnectionType::Outbound, ConnectionType::Outbound)]
    #[case(ConnectionType::Inbound, ConnectionType::Outbound)]
    #[case(ConnectionType::Outbound, ConnectionType::Inbound)]
    fn denies_second_connection_to_same_peer(
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
    fn allows_connections_to_different_peers() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id_1 = get_peer_id(1);
        let peer_id_2 = get_peer_id(2);
        let connection_id_1 = get_connection_id(1);
        let connection_id_2 = get_connection_id(2);

        // Establish connections to different peers
        let result1 = establish_connection(
            &mut behaviour,
            ConnectionType::Inbound,
            peer_id_1,
            connection_id_1,
            0,
        );
        let result2 = establish_connection(
            &mut behaviour,
            ConnectionType::Outbound,
            peer_id_2,
            connection_id_2,
            0,
        );

        result1.unwrap();
        result2.unwrap();
        assert!(behaviour.connected_peers().contains(&peer_id_1));
        assert!(behaviour.connected_peers().contains(&peer_id_2));
        assert_eq!(behaviour.connected_peers().len(), 2);
    }
}

// =============================================================================
// CONNECTION LIFECYCLE TESTS
// =============================================================================

/// Tests for connection establishment, closure, and failure scenarios
mod connection_lifecycle {
    use super::*;

    #[rstest]
    #[case(0)] // Normal close
    fn connection_closed_removes_peer(#[case] remaining_established: usize) {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);
        let connection_id = get_connection_id(1);

        // Establish connection
        let result = establish_connection(
            &mut behaviour,
            ConnectionType::Inbound,
            peer_id,
            connection_id,
            0,
        );
        result.unwrap();
        assert!(behaviour.connected_peers().contains(&peer_id));

        // Close connection
        close_connection(&mut behaviour, peer_id, connection_id, remaining_established);

        // Peer should be removed regardless of remaining_established value
        assert!(!behaviour.connected_peers().contains(&peer_id));
    }

    #[test]
    fn reconnection_after_close() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);
        let connection_id_1 = get_connection_id(1);
        let connection_id_2 = get_connection_id(2);

        // Establish, close, then re-establish connection
        let result1 = establish_connection(
            &mut behaviour,
            ConnectionType::Inbound,
            peer_id,
            connection_id_1,
            0,
        );
        result1.unwrap();

        close_connection(&mut behaviour, peer_id, connection_id_1, 0);
        assert!(!behaviour.connected_peers().contains(&peer_id));

        let result2 = establish_connection(
            &mut behaviour,
            ConnectionType::Outbound,
            peer_id,
            connection_id_2,
            0,
        );
        result2.unwrap();
        assert!(behaviour.connected_peers().contains(&peer_id));
    }

    #[rstest]
    #[case(FailureType::Dial, Some(get_peer_id(1)))] // Dial failure with peer_id
    #[case(FailureType::Dial, None)] // Dial failure without peer_id
    #[case(FailureType::Listen, Some(get_peer_id(1)))] // Listen failure with peer_id
    #[case(FailureType::Listen, None)] // Listen failure without peer_id
    fn connection_failures_do_not_remove_established_peers(
        #[case] failure_type: FailureType,
        #[case] peer_id: Option<PeerId>,
    ) {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let connection_id = get_connection_id(1);

        let initial_connected_count = if let Some(peer_id) = peer_id {
            // If we have a peer_id, establish a connection first
            let result = establish_connection(
                &mut behaviour,
                ConnectionType::Inbound,
                peer_id,
                connection_id,
                0,
            );
            result.unwrap();
            assert!(behaviour.connected_peers().contains(&peer_id));
            1
        } else {
            0
        };

        // Simulate failure
        fail_connection(&mut behaviour, peer_id, connection_id, failure_type);

        // Connection failures (DialFailure/ListenFailure) do NOT remove peers from connected_peers
        // They only remove pending connections. Only ConnectionClosed removes from connected_peers.
        if let Some(peer_id) = peer_id {
            if initial_connected_count == 1 {
                // Peer should still be connected - failures don't remove established connections
                assert!(behaviour.connected_peers().contains(&peer_id));
            }
        }
        // The connected set size should remain the same as initial
        assert_eq!(behaviour.connected_peers().len(), initial_connected_count);
    }

    #[rstest]
    #[case(0)] // Normal case
    #[case(1)] // Edge case: other_established > 0
    fn connection_established_with_other_established(#[case] other_established: usize) {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);
        let connection_id = get_connection_id(1);

        let result = establish_connection(
            &mut behaviour,
            ConnectionType::Inbound,
            peer_id,
            connection_id,
            other_established,
        );

        // Connection should be accepted regardless of other_established value
        result.unwrap();
        assert!(behaviour.connected_peers().contains(&peer_id));
    }

    #[test]
    fn only_connection_closed_removes_peers() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);
        let connection_id = get_connection_id(1);

        // Connection attempt doesn't add peer to connected_peers yet
        let result =
            attempt_connection(&mut behaviour, ConnectionType::Inbound, peer_id, connection_id);
        result.unwrap();
        assert!(!behaviour.connected_peers().contains(&peer_id));

        // ConnectionEstablished event adds peer to connected_peers
        simulate_connection_established(
            &mut behaviour,
            ConnectionType::Inbound,
            peer_id,
            connection_id,
            0,
        );
        assert!(behaviour.connected_peers().contains(&peer_id));

        // Only ConnectionClosed removes the peer
        close_connection(&mut behaviour, peer_id, connection_id, 0);
        assert!(!behaviour.connected_peers().contains(&peer_id));
        assert!(behaviour.connected_peers().is_empty());
    }
}

// =============================================================================
// EDGE CASE TESTS
// =============================================================================

/// Tests for complex scenarios and edge cases
mod edge_cases {
    use super::*;

    #[test]
    fn connection_denied_without_swarm_event() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);
        let connection_id_1 = get_connection_id(1);
        let connection_id_2 = get_connection_id(2);

        // Establish first connection (with swarm event)
        let result1 =
            attempt_connection(&mut behaviour, ConnectionType::Inbound, peer_id, connection_id_1);
        result1.unwrap();
        simulate_connection_established(
            &mut behaviour,
            ConnectionType::Inbound,
            peer_id,
            connection_id_1,
            0,
        );
        assert!(behaviour.connected_peers().contains(&peer_id));

        // Try to establish second connection to same peer (should be denied immediately)
        let addr = get_multiaddr();
        let result2 =
            behaviour.handle_established_inbound_connection(connection_id_2, peer_id, &addr, &addr);

        assert!(result2.is_err(), "Expected second connection to be denied");

        // Peer should still be connected (no change)
        assert!(behaviour.connected_peers().contains(&peer_id));
        assert_eq!(behaviour.connected_peers().len(), 1);
    }

    #[test]
    fn swarm_event_without_prior_connection_attempt() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);
        let connection_id = get_connection_id(1);

        // Simulate ConnectionEstablished event without prior connection attempt
        // This would happen if the event arrives without our handler being called first
        simulate_connection_established(
            &mut behaviour,
            ConnectionType::Inbound,
            peer_id,
            connection_id,
            0,
        );

        // ConnectionEstablished events add peers to connected_peers
        assert!(behaviour.connected_peers().contains(&peer_id));
        assert_eq!(behaviour.connected_peers().len(), 1);
    }

    #[test]
    fn multiple_connection_attempts_before_swarm_events() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id_1 = get_peer_id(1);
        let peer_id_2 = get_peer_id(2);
        let connection_id_1 = get_connection_id(1);
        let connection_id_2 = get_connection_id(2);

        // Attempt multiple connections without swarm events
        let result1 =
            attempt_connection(&mut behaviour, ConnectionType::Inbound, peer_id_1, connection_id_1);
        let result2 = attempt_connection(
            &mut behaviour,
            ConnectionType::Outbound,
            peer_id_2,
            connection_id_2,
        );

        result1.unwrap();
        result2.unwrap();

        // Peers are not added to connected_peers until ConnectionEstablished events
        assert!(!behaviour.connected_peers().contains(&peer_id_1));
        assert!(!behaviour.connected_peers().contains(&peer_id_2));
        assert_eq!(behaviour.connected_peers().len(), 0);

        // Swarm events add peers to connected_peers
        simulate_connection_established(
            &mut behaviour,
            ConnectionType::Inbound,
            peer_id_1,
            connection_id_1,
            0,
        );
        simulate_connection_established(
            &mut behaviour,
            ConnectionType::Outbound,
            peer_id_2,
            connection_id_2,
            0,
        );

        // Now peers should be in connected_peers after swarm events
        assert!(behaviour.connected_peers().contains(&peer_id_1));
        assert!(behaviour.connected_peers().contains(&peer_id_2));
        assert_eq!(behaviour.connected_peers().len(), 2);
    }

    #[test]
    fn multiple_attempt_connection_calls_same_peer() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);
        let connection_id_1 = get_connection_id(1);
        let connection_id_2 = get_connection_id(2);
        let connection_id_3 = get_connection_id(3);

        // First attempt should succeed
        let result1 =
            attempt_connection(&mut behaviour, ConnectionType::Inbound, peer_id, connection_id_1);
        result1.unwrap();
        // Peer not in connected_peers until ConnectionEstablished event
        assert!(!behaviour.connected_peers().contains(&peer_id));
        assert_eq!(behaviour.connected_peers().len(), 0);

        // Second attempt with same peer should fail (peer has pending connection)
        let result2 =
            attempt_connection(&mut behaviour, ConnectionType::Inbound, peer_id, connection_id_2);
        assert!(result2.is_err(), "Expected second connection attempt to be denied");

        // Third attempt with different connection type should also fail
        let result3 =
            attempt_connection(&mut behaviour, ConnectionType::Outbound, peer_id, connection_id_3);
        assert!(result3.is_err(), "Expected third connection attempt to be denied");

        // Peer should still not be in connected set (no ConnectionEstablished event yet)
        assert!(!behaviour.connected_peers().contains(&peer_id));
        assert_eq!(behaviour.connected_peers().len(), 0);
    }

    #[test]
    fn rapid_fire_connection_attempts_same_peer() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);

        // Rapid-fire connection attempts with different connection IDs
        let mut successful_attempts = 0;
        let mut failed_attempts = 0;

        for i in 1..=10 {
            let connection_id = get_connection_id(i);
            let connection_type =
                if i % 2 == 0 { ConnectionType::Inbound } else { ConnectionType::Outbound };

            match attempt_connection(&mut behaviour, connection_type, peer_id, connection_id) {
                Ok(_) => {
                    successful_attempts += 1;
                    println!("Connection attempt {} succeeded", i);
                }
                Err(_) => {
                    failed_attempts += 1;
                    println!("Connection attempt {} failed (expected)", i);
                }
            }

            if i == 5 {
                // Simulate a swarm event after 5 attempts
                fail_connection(&mut behaviour, Some(peer_id), connection_id, FailureType::Listen);
            }
            if i == 7 {
                // Simulate a swarm event after 5 attempts
                simulate_connection_established(
                    &mut behaviour,
                    connection_type,
                    peer_id,
                    connection_id,
                    0,
                );
            }
        }

        // Only the first attempt should succeed
        assert_eq!(successful_attempts, 1, "Expected exactly 1 successful connection attempt");
        assert_eq!(failed_attempts, 9, "Expected 9 failed connection attempts");

        // Peer should still be in connected set only once
        assert!(behaviour.connected_peers().contains(&peer_id));
        assert_eq!(behaviour.connected_peers().len(), 1);
    }

    #[test]
    fn attempt_connection_after_close_without_swarm_events() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);
        let connection_id_1 = get_connection_id(1);
        let connection_id_2 = get_connection_id(2);

        // First connection attempt
        let result1 =
            attempt_connection(&mut behaviour, ConnectionType::Inbound, peer_id, connection_id_1);
        result1.unwrap();
        // Peer not added to connected_peers until ConnectionEstablished event
        assert!(!behaviour.connected_peers().contains(&peer_id));

        // Simulate ConnectionEstablished to actually add peer to connected_peers
        simulate_connection_established(
            &mut behaviour,
            ConnectionType::Inbound,
            peer_id,
            connection_id_1,
            0,
        );
        assert!(behaviour.connected_peers().contains(&peer_id));

        // Close connection (this removes peer from connected set)
        close_connection(&mut behaviour, peer_id, connection_id_1, 0);
        assert!(!behaviour.connected_peers().contains(&peer_id));

        // Now try another connection attempt - this should succeed since peer was removed
        let result2 =
            attempt_connection(&mut behaviour, ConnectionType::Outbound, peer_id, connection_id_2);
        result2.unwrap();
        // Peer not in connected_peers until ConnectionEstablished event
        assert!(!behaviour.connected_peers().contains(&peer_id));
        assert_eq!(behaviour.connected_peers().len(), 0);
    }
}

// =============================================================================
// INVARIANT TESTS
// =============================================================================

/// Tests for behavior guarantees and invariants
mod invariants {
    use super::*;

    #[test]
    fn behaviour_poll_always_returns_pending() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());

        // Poll should always return pending
        assert!(matches!(behaviour.poll(&mut cx), Poll::Pending));

        // Behaviour should never generate events
        assert_no_event(&mut behaviour);
    }

    #[test]
    fn behaviour_generates_no_events_during_lifecycle() {
        let mut behaviour = OneConnectionPerPeerBehaviour::default();
        let peer_id = get_peer_id(1);
        let connection_id = get_connection_id(1);

        // Should not generate events at any point in connection lifecycle
        assert_no_event(&mut behaviour);

        let _ = establish_connection(
            &mut behaviour,
            ConnectionType::Inbound,
            peer_id,
            connection_id,
            0,
        );
        assert_no_event(&mut behaviour);

        close_connection(&mut behaviour, peer_id, connection_id, 0);
        assert_no_event(&mut behaviour);
    }
}
