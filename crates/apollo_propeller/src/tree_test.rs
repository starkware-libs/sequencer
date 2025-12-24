use libp2p::PeerId;

use crate::types::PeerSetError;
use crate::PropellerScheduleManager;

#[test]
fn test_create_empty_schedule_manager() {
    let peer = PeerId::random();
    PropellerScheduleManager::new(peer, vec![]).unwrap_err();
}

#[test]
fn test_create_schedule_manager() {
    let peer = PeerId::random();
    let manager = PropellerScheduleManager::new(peer, vec![(peer, 100)]).unwrap();
    assert_eq!(manager.get_local_peer_id(), peer);
    assert_eq!(manager.get_node_count(), 1);
}

#[test]
fn test_manager_creation_and_calculations() {
    let peer1 = PeerId::random();
    let peer2 = PeerId::random();
    let peer3 = PeerId::random();
    let peer4 = PeerId::random();

    let manager = PropellerScheduleManager::new(
        peer1,
        vec![(peer1, 100), (peer2, 75), (peer3, 50), (peer4, 25)],
    )
    .unwrap();

    assert_eq!(manager.get_node_count(), 4);
    // num_data_shards = floor((4-1)/3) = 1
    assert_eq!(manager.num_data_shards(), 1);
    // Coding shards = (N-1) - num_data_shards = 3 - 1 = 2
    assert_eq!(manager.num_coding_shards(), 2);
}

#[test]
fn test_should_build_and_receive() {
    let peer1 = PeerId::random();
    let peer2 = PeerId::random();
    let peer3 = PeerId::random();
    let peer4 = PeerId::random();

    let manager = PropellerScheduleManager::new(
        peer1,
        vec![(peer1, 100), (peer2, 75), (peer3, 50), (peer4, 25)],
    )
    .unwrap();

    // num_data_shards = 1, so should build with 1 shard
    assert!(manager.should_build(1));
    assert!(!manager.should_build(0));

    // Should receive with 2*num_data_shards = 2 shards
    assert!(manager.should_receive(2));
    assert!(!manager.should_receive(1));
}

#[test]
fn test_new_schedule_manager_without_local_peer() {
    let peer1 = PeerId::random();
    let peer2 = PeerId::random();

    let result = PropellerScheduleManager::new(peer1, vec![(peer2, 100)]);
    assert_eq!(result.unwrap_err(), PeerSetError::LocalPeerNotInChannel);
}
