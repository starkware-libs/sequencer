use libp2p::PeerId;
use rstest::rstest;

use crate::types::{PeerSetError, ShardIndex, TreeGenerationError};
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

#[rstest]
#[case::shard_0_maps_to_peer1(ShardIndex(0), Ok(0))]
#[case::shard_1_maps_to_peer3(ShardIndex(1), Ok(2))]
#[case::shard_2_maps_to_peer4(ShardIndex(2), Ok(3))]
#[case::shard_3_out_of_bounds(ShardIndex(3), Err(TreeGenerationError::ShardIndexOutOfBounds { shard_index: ShardIndex(3) }))]
#[case::shard_4_out_of_bounds(ShardIndex(4), Err(TreeGenerationError::ShardIndexOutOfBounds { shard_index: ShardIndex(4) }))]
fn test_get_peer_for_shard_id(
    #[case] shard_index: ShardIndex,
    #[case] expected_result: Result<usize, TreeGenerationError>,
) {
    let mut peers: Vec<_> = (0..4).map(|_| PeerId::random()).collect();
    peers.sort();
    let (peer1, peer2, peer3, peer4) = (peers[0], peers[1], peers[2], peers[3]);

    let manager = PropellerScheduleManager::new(
        peer1,
        vec![(peer1, 100), (peer2, 75), (peer3, 50), (peer4, 25)],
    )
    .unwrap();

    let publisher = peer2; // Use the second peer as publisher

    let result = manager.get_peer_for_shard_id(&publisher, shard_index);
    match expected_result {
        Ok(peer_index) => assert_eq!(result.unwrap(), peers[peer_index]),
        Err(expected_error) => assert_eq!(result.unwrap_err(), expected_error),
    }
}
