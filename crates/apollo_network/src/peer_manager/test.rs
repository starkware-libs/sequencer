// TODO(shahak): Add tests for multiple connection ids

use core::{panic, time};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use apollo_network_types::test_utils::DUMMY_PEER_ID;
use assert_matches::assert_matches;
use futures::future::poll_fn;
use futures::{FutureExt, Stream, StreamExt};
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{ConnectionId, NetworkBehaviour, ToSwarm};
use libp2p::{Multiaddr, PeerId};
use tokio::time::sleep;
use void::Void;

use super::behaviour_impl::ToOtherBehaviourEvent;
use crate::discovery::identify_impl::IdentifyToOtherBehaviourEvent;
use crate::misconduct_score::MisconductScore;
use crate::mixed_behaviour;
use crate::mixed_behaviour::BridgedBehaviour;
use crate::peer_manager::peer::Peer;
use crate::peer_manager::{PeerManager, PeerManagerConfig, ReputationModifier};
use crate::sqmr::OutboundSessionId;

impl Unpin for PeerManager {}

impl Stream for PeerManager {
    type Item = ToSwarm<ToOtherBehaviourEvent, Void>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

fn simulate_connection_established(
    peer_manager: &mut PeerManager,
    peer_id: PeerId,
    connection_id: ConnectionId,
) {
    peer_manager.on_swarm_event(libp2p::swarm::FromSwarm::ConnectionEstablished(
        ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint: &libp2p::core::ConnectedPoint::Dialer {
                address: Multiaddr::empty(),
                role_override: libp2p::core::Endpoint::Dialer,
            },
            failed_addresses: &[],
            other_established: 0,
        },
    ));
}

fn get_peer_id(index: u8) -> PeerId {
    // Generate a PeerId based on the index
    let key = [index; 32];
    let keypair = libp2p::identity::Keypair::ed25519_from_bytes(key).unwrap();
    PeerId::from_public_key(&keypair.public())
}

#[test]
fn peer_assignment_round_robin() {
    // Create a new peer manager
    let mut peer_manager = PeerManager::new(PeerManagerConfig::default());

    // Add two peers to the peer manager
    let peer1 = Peer::new(get_peer_id(1), Multiaddr::empty());
    let peer2 = Peer::new(get_peer_id(2), Multiaddr::empty());
    let connection_id1 = ConnectionId::new_unchecked(1);
    let connection_id2 = ConnectionId::new_unchecked(2);
    peer_manager.add_peer(peer1.clone());
    peer_manager.add_peer(peer2.clone());

    // Add connections to those peers.
    simulate_connection_established(&mut peer_manager, peer1.peer_id(), connection_id1);
    simulate_connection_established(&mut peer_manager, peer2.peer_id(), connection_id2);

    // Create three queries
    let session1 = OutboundSessionId { value: 1 };
    let session2 = OutboundSessionId { value: 2 };
    let session3 = OutboundSessionId { value: 3 };

    // Assign peers to the queries in a round-robin fashion
    let res1 = peer_manager.assign_peer_to_session(session1);
    let res2 = peer_manager.assign_peer_to_session(session2);
    let res3 = peer_manager.assign_peer_to_session(session3);

    // Verify that the peers are assigned in a round-robin fashion
    let is_peer1_first: bool;
    match res1.unwrap() {
        peer_id if peer_id == peer1.peer_id() => {
            is_peer1_first = true;
            assert_eq!(res2.unwrap(), peer2.peer_id());
            assert_eq!(res3.unwrap(), peer1.peer_id());
        }
        peer_id if peer_id == peer2.peer_id() => {
            is_peer1_first = false;
            assert_eq!(res2.unwrap(), peer1.peer_id());
            assert_eq!(res3.unwrap(), peer2.peer_id());
        }
        peer_id => panic!("Unexpected peer_id: {:?}", peer_id),
    }

    // check assignment events
    while let Some(event) = peer_manager.next().now_or_never() {
        let Some(ToSwarm::GenerateEvent(ToOtherBehaviourEvent::SessionAssigned {
            outbound_session_id,
            peer_id,
            connection_id,
        })) = event
        else {
            continue;
        };
        if is_peer1_first {
            match outbound_session_id {
                OutboundSessionId { value: 1 } => {
                    assert_eq!(peer_id, peer1.peer_id());
                    assert_eq!(connection_id, connection_id1)
                }
                OutboundSessionId { value: 2 } => {
                    assert_eq!(peer_id, peer2.peer_id());
                    assert_eq!(connection_id, connection_id2);
                }
                OutboundSessionId { value: 3 } => {
                    assert_eq!(peer_id, peer1.peer_id());
                    assert_eq!(connection_id, connection_id1);
                }
                _ => panic!("Unexpected outbound_session_id: {:?}", outbound_session_id),
            }
        } else {
            match outbound_session_id {
                OutboundSessionId { value: 1 } => {
                    assert_eq!(peer_id, peer2.peer_id());
                    assert_eq!(connection_id, connection_id2);
                }
                OutboundSessionId { value: 2 } => {
                    assert_eq!(peer_id, peer1.peer_id());
                    assert_eq!(connection_id, connection_id1);
                }
                OutboundSessionId { value: 3 } => {
                    assert_eq!(peer_id, peer2.peer_id());
                    assert_eq!(connection_id, connection_id2);
                }
                _ => panic!("Unexpected outbound_session_id: {:?}", outbound_session_id),
            }
        }
    }
}

#[tokio::test]
async fn peer_assignment_no_peers() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager = PeerManager::new(config.clone());

    // Create a session
    let outbound_session_id = OutboundSessionId { value: 1 };

    // Assign a peer to the session
    assert_matches!(peer_manager.assign_peer_to_session(outbound_session_id), None);
    assert!(peer_manager.next().now_or_never().is_none());

    // Now the peer manager finds a new peer and can assign the session.
    let connection_id = ConnectionId::new_unchecked(0);
    let peer_id = *DUMMY_PEER_ID;
    let mut peer = Peer::new(peer_id, Multiaddr::empty());
    peer.add_connection_id(connection_id);
    peer_manager.add_peer(peer);
    assert_matches!(
        peer_manager.next().await.unwrap(),
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::SessionAssigned {
                outbound_session_id: event_outbound_session_id,
                peer_id: event_peer_id,
                connection_id: event_connection_id,
            }
        ) if outbound_session_id == event_outbound_session_id &&
            peer_id == event_peer_id &&
            connection_id == event_connection_id
    );
    assert!(peer_manager.next().now_or_never().is_none());
}

#[tokio::test]
async fn peer_assignment_no_unblocked_peers() {
    const BLOCKED_UNTIL: Duration = Duration::from_secs(5);
    const TIMEOUT: Duration = Duration::from_secs(1);
    // Create a new peer manager
    let config =
        PeerManagerConfig { malicious_timeout_seconds: TIMEOUT, unstable_timeout_millis: TIMEOUT };
    let mut peer_manager: PeerManager = PeerManager::new(config.clone());

    // Create a session
    let outbound_session_id = OutboundSessionId { value: 1 };

    // Create a peer
    let connection_id = ConnectionId::new_unchecked(0);
    let peer_id = *DUMMY_PEER_ID;
    let mut peer = Peer::new(peer_id, Multiaddr::empty());
    peer.add_connection_id(connection_id);

    peer_manager.add_peer(peer);
    peer_manager.report_peer(peer_id, ReputationModifier::Unstable).unwrap();

    // Consume the peer blacklisted event
    let event = tokio::time::timeout(TIMEOUT, peer_manager.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::PeerBlacklisted { peer_id: event_peer_id })
        if peer_id == event_peer_id
    );

    // Try to assign a peer to the session, and check there wasn't any assignment.
    assert_matches!(peer_manager.assign_peer_to_session(outbound_session_id), None);
    assert!(peer_manager.next().now_or_never().is_none());

    // Simulate that BLOCKED_UNTIL has passed.
    tokio::time::pause();
    tokio::time::advance(BLOCKED_UNTIL).await;
    tokio::time::resume();

    // After BLOCKED_UNTIL has passed, the peer manager can assign the session.
    let event = tokio::time::timeout(TIMEOUT, peer_manager.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::SessionAssigned {
                outbound_session_id: event_outbound_session_id,
                peer_id: event_peer_id,
                connection_id: event_connection_id,
            }
        ) if outbound_session_id == event_outbound_session_id &&
            peer_id == event_peer_id &&
            connection_id == event_connection_id
    );
    assert!(peer_manager.next().now_or_never().is_none());
}

#[test]
fn report_peer_calls_update_reputation_and_notifies_kad() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager = PeerManager::new(config.clone());

    // Create a peer
    let peer_id = *DUMMY_PEER_ID;
    let peer = Peer::new(peer_id, Multiaddr::empty());

    peer_manager.add_peer(peer);

    // Call the report_peer function on the peer manager
    peer_manager.report_peer(peer_id, ReputationModifier::Unstable).unwrap();
    peer_manager.get_mut_peer(peer_id).unwrap();

    // Validate that we have an event to notify Kademlia
    assert_eq!(peer_manager.pending_events.len(), 1);
    assert_matches!(
        peer_manager.pending_events.first().unwrap(),
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::PeerBlacklisted { peer_id: event_peer_id })
        if peer_id == *event_peer_id
    );
}

#[tokio::test]
async fn peer_block_released_after_timeout() {
    const DURATION_IN_MILLIS: u64 = 50;
    let mut peer = Peer::new(*DUMMY_PEER_ID, Multiaddr::empty());
    peer.blacklist_peer(Duration::from_millis(DURATION_IN_MILLIS));
    assert!(peer.is_blocked());
    sleep(time::Duration::from_millis(DURATION_IN_MILLIS)).await;
    assert!(!peer.is_blocked());
}

#[test]
fn report_peer_on_unknown_peer_id() {
    // Create a new peer manager
    let mut peer_manager: PeerManager = PeerManager::new(PeerManagerConfig::default());

    // report peer on an unknown peer_id
    let peer_id = *DUMMY_PEER_ID;
    peer_manager
        .report_peer(peer_id, ReputationModifier::Unstable {})
        .expect_err("report_peer on unknown peer_id should return an error");
}

#[test]
fn report_session_calls_update_reputation() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager = PeerManager::new(config.clone());

    // Create a peer
    let peer_id = *DUMMY_PEER_ID;
    let mut peer = Peer::new(peer_id, Multiaddr::empty());
    peer.add_connection_id(ConnectionId::new_unchecked(0));

    // Add the peer to the peer manager
    peer_manager.add_peer(peer);

    // Create a session
    let outbound_session_id = OutboundSessionId { value: 1 };

    // Assign peer to the session
    let res_peer_id = peer_manager.assign_peer_to_session(outbound_session_id).unwrap();
    assert_eq!(res_peer_id, peer_id);

    // Call the report_peer function on the peer manager
    peer_manager.report_session(outbound_session_id, ReputationModifier::Unstable {}).unwrap();
    peer_manager.get_mut_peer(peer_id).unwrap();
}

#[test]
fn report_session_on_unknown_session_id() {
    // Create a new peer manager
    let mut peer_manager: PeerManager = PeerManager::new(PeerManagerConfig::default());

    // Create a session
    let outbound_session_id = OutboundSessionId { value: 1 };

    peer_manager
        .report_session(
            outbound_session_id,
            ReputationModifier::Misconduct { misconduct_score: MisconductScore::MALICIOUS },
        )
        .expect_err("report_session on unknown outbound_session_id should return an error");
}

#[tokio::test]
async fn timed_out_peer_not_assignable_to_queries() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager = PeerManager::new(config.clone());

    // Create a peer
    let peer_id = *DUMMY_PEER_ID;
    let peer = Peer::new(peer_id, Multiaddr::empty());

    // Add the peer to the peer manager
    peer_manager.add_peer(peer);

    // Report the peer as bad
    peer_manager
        .report_peer(
            peer_id,
            ReputationModifier::Misconduct { misconduct_score: MisconductScore::MALICIOUS },
        )
        .unwrap();

    // Create a session
    let outbound_session_id = OutboundSessionId { value: 1 };

    // Assign peer to the session
    assert_matches!(peer_manager.assign_peer_to_session(outbound_session_id), None);
}

#[test]
fn wrap_around_in_peer_assignment() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager = PeerManager::new(config.clone());

    // Create a peer
    let peer_id1 = get_peer_id(1);
    let mut peer1 = Peer::new(peer_id1, Multiaddr::empty());
    peer1.add_connection_id(ConnectionId::new_unchecked(0));

    // Add the peer to the peer manager
    peer_manager.add_peer(peer1);

    // Report the peer as malicious
    peer_manager
        .report_peer(
            peer_id1,
            ReputationModifier::Misconduct { misconduct_score: MisconductScore::MALICIOUS },
        )
        .unwrap();

    // Create a peer
    let peer_id2 = get_peer_id(2);
    let mut peer2 = Peer::new(peer_id2, Multiaddr::empty());
    peer2.add_connection_id(ConnectionId::new_unchecked(0));

    // Add the peer to the peer manager
    peer_manager.add_peer(peer2);

    // Create a session
    let outbound_session_id = OutboundSessionId { value: 1 };

    // Assign peer to the session - since we don't know what is the order of the peers in the
    // HashMap, we need to assign twice to make sure we wrap around
    assert_matches!(peer_manager.assign_peer_to_session(outbound_session_id), Some(peer_id) if peer_id == peer_id2);
    assert_matches!(peer_manager.assign_peer_to_session(outbound_session_id), Some(peer_id) if peer_id == peer_id2);
}

#[test]
fn block_and_allow_inbound_connection() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager = PeerManager::new(config.clone());

    // Create a peer - report as malicious
    let peer_id1 = get_peer_id(1);
    let peer1 = Peer::new(peer_id1, Multiaddr::empty());

    // Create a peer - not blocked
    let peer_id2 = get_peer_id(2);
    let peer2 = Peer::new(peer_id2, Multiaddr::empty());

    peer_manager.add_peer(peer1);
    peer_manager.add_peer(peer2);

    peer_manager
        .report_peer(
            peer_id1,
            ReputationModifier::Misconduct { misconduct_score: MisconductScore::MALICIOUS },
        )
        .unwrap();

    // call handle_established_inbound_connection with the blocked peer
    let res = peer_manager.handle_established_inbound_connection(
        libp2p::swarm::ConnectionId::new_unchecked(0),
        peer_id1,
        &Multiaddr::empty(),
        &Multiaddr::empty(),
    );
    // ConnectionHandler doesn't implement Debug so we have to assert the result like that.
    assert!(res.is_err());

    // call handle_established_inbound_connection with the blocked peer
    let res = peer_manager.handle_established_inbound_connection(
        libp2p::swarm::ConnectionId::new_unchecked(0),
        peer_id2,
        &Multiaddr::empty(),
        &Multiaddr::empty(),
    );
    // ConnectionHandler doesn't implement Debug so we have to assert the result like that.
    assert!(res.is_ok());
}

#[tokio::test]
async fn if_all_peers_have_no_connection_assign_only_once_a_peer_connects() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager = PeerManager::new(config.clone());

    // Create a peer
    let peer_id = *DUMMY_PEER_ID;
    let peer = Peer::new(peer_id, Multiaddr::empty());

    // Add the peer to the peer manager
    peer_manager.add_peer(peer);

    // Create a session
    let outbound_session_id = OutboundSessionId { value: 1 };

    // Assign peer to the session and make sure assignment didn't return a peer.
    assert!(peer_manager.assign_peer_to_session(outbound_session_id).is_none());

    // Add a connection to the peer
    simulate_connection_established(&mut peer_manager, peer_id, ConnectionId::new_unchecked(0));

    // Expect SessionAssigned event
    assert_matches!(
        poll_fn(|cx| peer_manager.poll(cx)).await,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::SessionAssigned {
            peer_id: actual_peer_id, ..
        }) if peer_id == actual_peer_id
    );
}

#[test]
fn identify_on_unknown_peer_is_added_to_peer_manager() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager = PeerManager::new(config.clone());

    // Send Identify event
    let peer_id = *DUMMY_PEER_ID;
    let address = Multiaddr::empty().with_p2p(peer_id).unwrap();
    peer_manager.on_other_behaviour_event(&mixed_behaviour::ToOtherBehaviourEvent::Identify(
        IdentifyToOtherBehaviourEvent::FoundListenAddresses {
            peer_id,
            listen_addresses: vec![address.clone()],
        },
    ));

    // Check that the peer is added to the peer manager
    let res_peer_id = peer_manager.get_mut_peer(peer_id).unwrap();
    assert!(res_peer_id.peer_id() == peer_id);
    assert!(res_peer_id.multiaddr() == address);
}
