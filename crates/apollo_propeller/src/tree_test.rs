use libp2p::PeerId;
use rstest::rstest;

use crate::types::{PeerSetError, ShardIndex, TreeGenerationError};
use crate::{PropellerScheduleManager, ShardValidationError};

// TODO(AndrewL): Move this to test_utils crate.
pub fn get_peer_id(index: u8) -> PeerId {
    // Generate a PeerId based on the index
    let key = [index; 32];
    let keypair = libp2p::identity::Keypair::ed25519_from_bytes(key).unwrap();
    PeerId::from_public_key(&keypair.public())
}

fn make_schedule_manager(index: u8, num_nodes: u8) -> PropellerScheduleManager {
    let mut peers: Vec<_> = (0..num_nodes).map(get_peer_id).collect();
    peers.sort();
    let local_peer = peers[usize::from(index)];
    let scheduler = PropellerScheduleManager::new(
        local_peer,
        peers.into_iter().map(|peer| (peer, 1)).collect(),
    )
    .unwrap();
    assert_eq!(scheduler.get_nodes()[usize::from(index)].0, local_peer);
    scheduler
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
fn test_manager_creation_and_calculations() {
    let schedule_manager = make_schedule_manager(0, 4);
    assert_eq!(schedule_manager.get_node_count(), 4);
    // num_data_shards = floor((4-1)/3) = 1
    assert_eq!(schedule_manager.num_data_shards(), 1);
    // Coding shards = (N-1) - num_data_shards = 3 - 1 = 2
    assert_eq!(schedule_manager.num_coding_shards(), 2);
}

#[rstest]
#[case::two_nodes(2, 1, 1)]
#[case::three_nodes(3, 1, 1)]
#[case::four_nodes(4, 1, 2)]
#[case::seven_nodes(7, 2, 4)]
#[case::ten_nodes(10, 3, 6)]
#[case::thirteen_nodes(13, 4, 8)]
fn test_should_build_and_receive(
    #[case] num_nodes: u8,
    #[case] build_threshold: usize,
    #[case] receive_threshold: usize,
) {
    let schedule_manager = make_schedule_manager(0, num_nodes);
    assert_eq!(schedule_manager.num_data_shards(), build_threshold);
    assert!(schedule_manager.should_build(build_threshold));
    assert!(!schedule_manager.should_build(build_threshold - 1));
    assert!(schedule_manager.should_receive(receive_threshold));
    assert!(!schedule_manager.should_receive(receive_threshold - 1));
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
    #[case] shard_index: ShardIndex,
    #[case] expected_result: Result<usize, TreeGenerationError>,
) {
    let schedule_manager = make_schedule_manager(0, 4);
    let publisher = schedule_manager.get_nodes()[1].0; // Use peer1 as publisher
    let result = schedule_manager.get_peer_for_shard_index(&publisher, shard_index);
    assert_eq!(
        result,
        expected_result.map(|peer_index| schedule_manager.get_nodes()[peer_index].0)
    );
}

fn get_result_for_validate_origin(
    num_nodes: u8,
    local_index: u8,
    sender_index: u8,
    publisher_index: u8,
    shard_index: u32,
) -> Result<(), ShardValidationError> {
    let schedule_manager = make_schedule_manager(local_index, num_nodes);
    let get_peer = |i: u8| {
        if i == u8::MAX {
            get_peer_id(u8::MAX)
        } else {
            schedule_manager.get_nodes()[usize::from(i)].0
        }
    };
    let sender = get_peer(sender_index);
    let publisher = get_peer(publisher_index);
    schedule_manager.validate_origin(sender, publisher, ShardIndex(shard_index))
}

#[rstest]
#[case::two_peers_peer0_receives_from_peer1(2, 0, 1, 0)]
#[case::three_peers_peer0_receives_from_peer1(3, 0, 1, 0)]
#[case::four_peers_peer0_receives_from_peer1(4, 0, 1, 0)]
#[case::four_peers_peer2_receives_from_peer1(4, 2, 1, 1)]
#[case::four_peers_publisher_is_first(4, 2, 0, 1)]
#[case::four_peers_publisher_is_last(4, 0, 3, 0)]
#[case::seven_peers_peer2_receives_from_peer3(7, 2, 3, 2)]
#[case::ten_peers_peer5_receives_from_peer3(10, 5, 3, 4)]
#[case::hundred_peers_large_network(100, 50, 25, 49)]
fn test_validate_origin_my_shard_from_publisher(
    #[case] num_nodes: u8,
    #[case] local_index: u8,
    #[case] publisher_index: u8,
    #[case] shard_index: u32,
) {
    get_result_for_validate_origin(
        num_nodes,
        local_index,
        publisher_index,
        publisher_index,
        shard_index,
    )
    .unwrap();
}

#[rstest]
#[case::three_peers_relay(3, 0, 2, 1, 1)]
#[case::four_peers_relay_shard1_via_peer2(4, 0, 2, 1, 1)]
#[case::four_peers_relay_shard0_via_peer0(4, 2, 0, 1, 0)]
#[case::seven_peers_relay_shard1_via_peer1(7, 2, 1, 3, 1)]
#[case::ten_peers_relay_shard5_via_peer6(10, 5, 6, 3, 5)]
fn test_validate_origin_from_shard_owner(
    #[case] num_nodes: u8,
    #[case] local_index: u8,
    #[case] sender_index: u8,
    #[case] publisher_index: u8,
    #[case] shard_index: u32,
) {
    get_result_for_validate_origin(
        num_nodes,
        local_index,
        sender_index,
        publisher_index,
        shard_index,
    )
    .unwrap();
}

#[rstest]
#[case::self_send_two_peers(2, 0, 0, 1, 0)]
#[case::self_send_four_peers(4, 1, 1, 2, 0)]
#[case::self_publish_four_peers(4, 0, 1, 0, 0)]
#[case::wrong_sender_four_peers(4, 0, 2, 1, 0)]
#[case::wrong_sender_seven_peers(7, 2, 5, 3, 0)]
#[case::hop1_wrong_shard_four_peers(4, 0, 1, 1, 1)]
#[case::hop1_wrong_shard_seven_peers(7, 2, 1, 1, 0)]
#[case::malicious_publisher_wrong_shard(7, 3, 2, 2, 0)]
#[case::relay_attack_wrong_broadcaster(4, 2, 3, 1, 0)]
#[case::hop_confusion_should_relay(7, 0, 3, 3, 1)]
#[case::unknown_sender(4, 0, u8::MAX, 1, 0)]
#[case::unknown_publisher(4, 0, 1, u8::MAX, 0)]
#[case::shard_at_boundary(4, 0, 1, 1, 3)]
#[case::shard_just_over_boundary(4, 0, 1, 1, 4)]
#[case::shard_out_of_bounds(4, 0, 1, 1, 100)]
#[case::shard_index_u32_max(4, 0, 1, 1, u32::MAX)]
fn test_validate_origin_failures(
    #[case] num_nodes: u8,
    #[case] local_index: u8,
    #[case] sender_index: u8,
    #[case] publisher_index: u8,
    #[case] shard_index: u32,
) {
    get_result_for_validate_origin(
        num_nodes,
        local_index,
        sender_index,
        publisher_index,
        shard_index,
    )
    .unwrap_err();
}
