use std::collections::HashMap;

use libp2p::PeerId;
use rstest::{fixture, rstest};

use crate::types::{PeerSetError, ShardIndex, TreeGenerationError};
use crate::PropellerScheduleManager;

#[fixture]
fn schedule_manager() -> PropellerScheduleManager {
    let peers: Vec<_> = (0..4).map(|_| PeerId::random()).collect();
    let local_peer = peers[0];
    PropellerScheduleManager::new(local_peer, peers.iter().map(|peer| (*peer, 1)).collect())
        .unwrap()
}

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

#[rstest]
fn test_manager_creation_and_calculations(schedule_manager: PropellerScheduleManager) {
    assert_eq!(schedule_manager.get_node_count(), 4);
    // num_data_shards = floor((4-1)/3) = 1
    assert_eq!(schedule_manager.num_data_shards(), 1);
    // Coding shards = (N-1) - num_data_shards = 3 - 1 = 2
    assert_eq!(schedule_manager.num_coding_shards(), 2);
}

#[rstest]
fn test_should_build_and_receive(schedule_manager: PropellerScheduleManager) {
    // num_data_shards = 1, so should build with 1 shard
    assert!(schedule_manager.should_build(1));
    assert!(!schedule_manager.should_build(0));
    // Should receive with 2*num_data_shards = 2 shards
    assert!(schedule_manager.should_receive(2));
    assert!(!schedule_manager.should_receive(1));
}

#[test]
fn test_new_schedule_manager_without_local_peer() {
    let peer1 = PeerId::random();
    let peer2 = PeerId::random();
    let result = PropellerScheduleManager::new(peer1, vec![(peer2, 100)]);
    assert_eq!(result.unwrap_err(), PeerSetError::LocalPeerNotInChannel);
}

#[rstest]
#[case::shard_0_published_by_peer1_maps_to_peer0(ShardIndex(0), Ok(0))]
#[case::shard_1_published_by_peer1_maps_to_peer2(ShardIndex(1), Ok(2))]
#[case::shard_2_published_by_peer1_maps_to_peer3(ShardIndex(2), Ok(3))]
#[case::shard_3_out_of_bounds(ShardIndex(3), Err(TreeGenerationError::ShardIndexOutOfBounds { shard_index: ShardIndex(3) }))]
#[case::shard_4_out_of_bounds(ShardIndex(4), Err(TreeGenerationError::ShardIndexOutOfBounds { shard_index: ShardIndex(4) }))]
fn test_get_peer_for_shard_id(
    schedule_manager: PropellerScheduleManager,
    #[case] shard_index: ShardIndex,
    #[case] expected_result: Result<usize, TreeGenerationError>,
) {
    let publisher = schedule_manager.get_nodes()[1].0; // Use peer1 as publisher
    let result = schedule_manager.get_peer_for_shard_index(&publisher, shard_index);
    assert_eq!(
        result,
        expected_result.map(|peer_index| schedule_manager.get_nodes()[peer_index].0)
    );
}

#[rstest]
fn test_validate_origin(
    schedule_manager: PropellerScheduleManager,
    #[values(None, Some(0), Some(1), Some(2), Some(3))] sender_index: Option<usize>,
    #[values(None, Some(0), Some(1), Some(2), Some(3))] publisher_index: Option<usize>,
    #[values(0, 1, 2, 3, 5, 6, 10)] shard_index: u32,
) {
    let shard_index = ShardIndex(shard_index);
    let sender =
        sender_index.map(|j| schedule_manager.get_nodes()[j].0).unwrap_or_else(PeerId::random);
    let publisher =
        publisher_index.map(|j| schedule_manager.get_nodes()[j].0).unwrap_or_else(PeerId::random);
    let mut peer_to_index = HashMap::new();
    for (index, (peer, _)) in
        schedule_manager.get_nodes().iter().filter(|peer| peer.0 != publisher).enumerate()
    {
        peer_to_index.insert(peer, ShardIndex(index.try_into().unwrap()));
    }
    let result = schedule_manager.validate_origin(sender, publisher, shard_index);

    // Analyze the result
    let local_peer = schedule_manager.get_local_peer_id();
    let sender_in_channel = schedule_manager.get_nodes().iter().any(|peer| peer.0 == sender);
    let publisher_in_channel = schedule_manager.get_nodes().iter().any(|peer| peer.0 == publisher);
    let hop_1 = (sender == publisher) && (peer_to_index.get(&local_peer) == Some(&shard_index));
    let hop_2 = (sender != local_peer)
        && (publisher != local_peer)
        && (peer_to_index.get(&sender) == Some(&shard_index));

    if sender_in_channel && publisher_in_channel && (hop_1 || hop_2) {
        result.expect("Valid origin validation failed.");
    } else {
        result.expect_err("Invalid origin validation succeeded.");
    }
}
