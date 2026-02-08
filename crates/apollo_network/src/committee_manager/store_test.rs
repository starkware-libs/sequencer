use apollo_network_types::test_utils::get_peer_id;
use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;

use super::{CommitteeStore, CommitteeStoreError};
use crate::committee_manager::types::CommitteeMember;

fn staker(index: u8) -> ContractAddress {
    ContractAddress::from(index)
}

fn member(index: u8) -> CommitteeMember {
    CommitteeMember { staker_id: staker(index), weight: StakingWeight(1) }
}

fn committee_id(index: u8) -> Felt {
    Felt::from(index)
}

// ---------------------------------------------------------------------------
// Read API tests
// ---------------------------------------------------------------------------

#[test]
fn get_committee_returns_none_for_unknown_epoch() {
    let store = CommitteeStore::new(2);
    assert!(store.get_committee(&42).is_none());
}

#[test]
fn get_epoch_returns_none_for_unknown_committee_id() {
    let store = CommitteeStore::new(2);
    assert!(store.get_epoch(&committee_id(1)).is_none());
}

// ---------------------------------------------------------------------------
// add_committee tests
// ---------------------------------------------------------------------------

#[test]
fn add_committee_and_read_back() {
    let mut store = CommitteeStore::new(2);
    let members = vec![member(1), member(2)];
    let peers = store.add_committee(10, committee_id(1), members.clone()).unwrap();
    assert!(peers.is_empty());

    // get_committee by epoch
    let (cid, returned_members) = store.get_committee(&10).unwrap();
    assert_eq!(cid, committee_id(1));
    assert_eq!(returned_members, members.as_slice());

    // get_epoch by committee_id
    let (eid, returned_members) = store.get_epoch(&committee_id(1)).unwrap();
    assert_eq!(eid, 10);
    assert_eq!(returned_members, members.as_slice());
}

#[test]
fn add_committee_duplicate_epoch_fails() {
    let mut store = CommitteeStore::new(2);
    store.add_committee(10, committee_id(1), vec![member(1)]).unwrap();
    let result = store.add_committee(10, committee_id(2), vec![member(2)]);
    assert!(matches!(result, Err(CommitteeStoreError::EpochAlreadyExists(10))));
}

#[test]
fn add_committee_stakers_can_be_mapped() {
    let mut store = CommitteeStore::new(2);

    // Before adding a committee, the staker cannot be mapped.
    let result = store.add_peer_for_staker(staker(1), get_peer_id(1));
    assert!(matches!(result, Err(CommitteeStoreError::UnknownStaker(_))));

    // After adding a committee, the staker can be mapped.
    store.add_committee(10, committee_id(1), vec![member(1)]).unwrap();
    store.add_peer_for_staker(staker(1), get_peer_id(1)).unwrap();
}

// ---------------------------------------------------------------------------
// add_peer_for_staker tests
// ---------------------------------------------------------------------------
#[test]
fn add_peer_for_staker_succeeds() {
    let mut store = CommitteeStore::new(2);
    store.add_committee(10, committee_id(1), vec![member(1)]).unwrap();
    store.add_peer_for_staker(staker(2), get_peer_id(3)).unwrap();
    assert_eq!(store.staker_to_peer.get(&staker(2)).unwrap(), &get_peer_id(3));
    assert_eq!(store.peer_to_staker.get(&get_peer_id(3)).unwrap(), &staker(2));
}

#[test]
fn add_peer_for_staker_duplicate_peer_fails() {
    let mut store = CommitteeStore::new(2);
    store.add_committee(10, committee_id(1), vec![member(1)]).unwrap();

    let peer = get_peer_id(1);
    store.add_peer_for_staker(staker(1), peer).unwrap();

    // Staker already has a peer, so a second mapping should fail.
    let result = store.add_peer_for_staker(staker(1), get_peer_id(2));
    assert!(matches!(result, Err(CommitteeStoreError::StakerAlreadyMapped(_))));
}

#[test]
fn add_peer_for_unknown_staker_fails() {
    let mut store = CommitteeStore::new(2);
    let peer = get_peer_id(1);
    let result = store.add_peer_for_staker(staker(1), peer);
    assert!(matches!(result, Err(CommitteeStoreError::UnknownStaker(_))));
}

#[test]
fn add_peer_for_already_mapped_staker_fails() {
    let mut store = CommitteeStore::new(2);
    store.add_committee(10, committee_id(1), vec![member(1)]).unwrap();
    store.add_peer_for_staker(staker(1), get_peer_id(1)).unwrap();

    let result = store.add_peer_for_staker(staker(1), get_peer_id(2));
    assert!(matches!(result, Err(CommitteeStoreError::StakerAlreadyMapped(_))));
}

// ---------------------------------------------------------------------------
// remove_peer tests
// ---------------------------------------------------------------------------

#[test]
fn remove_peer_clears_mapping() {
    let mut store = CommitteeStore::new(2);
    store.add_committee(10, committee_id(1), vec![member(1)]).unwrap();
    let peer = get_peer_id(1);
    store.add_peer_for_staker(staker(1), peer).unwrap();

    store.remove_peer(&peer);

    // Staker can be mapped again after the previous peer was removed.
    store.add_peer_for_staker(staker(1), get_peer_id(2)).unwrap();
}

#[test]
fn remove_peer_noop_for_unknown_peer() {
    let mut store = CommitteeStore::new(2);
    // Should not panic.
    store.remove_peer(&get_peer_id(99));
}

// ---------------------------------------------------------------------------
// Eviction tests
// ---------------------------------------------------------------------------

#[test]
fn eviction_triggers_when_at_capacity() {
    let mut store = CommitteeStore::new(2);

    // Fill to capacity.
    store.add_committee(1, committee_id(1), vec![member(1)]).unwrap();
    store.add_committee(2, committee_id(2), vec![member(2)]).unwrap();

    // Add a peer for staker(1)
    store.add_peer_for_staker(staker(1), get_peer_id(1)).unwrap();

    // Adding a third should evict epoch 1.
    let peers = store.add_committee(3, committee_id(3), vec![member(3)]).unwrap();
    assert_eq!(peers, vec![get_peer_id(1)]); // staker(1) had a peer mapped.

    // Epoch 1 should be gone.
    assert!(store.get_committee(&1).is_none());
    assert!(store.get_epoch(&committee_id(1)).is_none());
    // Staker 1 was evicted, so mapping should fail.
    let result = store.add_peer_for_staker(staker(1), get_peer_id(1));
    assert!(matches!(result, Err(CommitteeStoreError::UnknownStaker(_))));

    // Epochs 2 and 3 should still be present.
    assert!(store.get_committee(&2).is_some());
    assert!(store.get_committee(&3).is_some());
}

#[test]
fn eviction_returns_peers_to_disconnect() {
    let mut store = CommitteeStore::new(1);

    store.add_committee(1, committee_id(1), vec![member(1)]).unwrap();
    let peer = get_peer_id(1);
    store.add_peer_for_staker(staker(1), peer).unwrap();

    // Adding epoch 2 should evict epoch 1, and staker(1) had a peer mapped.
    let peers = store.add_committee(2, committee_id(2), vec![member(2)]).unwrap();
    assert_eq!(peers, vec![peer]);
}

#[test]
fn eviction_does_not_disconnect_staker_still_in_another_epoch() {
    let mut store = CommitteeStore::new(2);

    // Staker 1 is in both epochs.
    store.add_committee(1, committee_id(1), vec![member(1)]).unwrap();
    store.add_committee(2, committee_id(2), vec![member(1), member(2)]).unwrap();
    store.add_peer_for_staker(staker(1), get_peer_id(1)).unwrap();

    // Evict epoch 1. Staker 1 still has ref count > 0 from epoch 2.
    let peers = store.add_committee(3, committee_id(3), vec![member(3)]).unwrap();
    assert!(peers.is_empty());

    // Staker 1 is still mapped (already has a peer, so a second mapping should fail).
    let result = store.add_peer_for_staker(staker(1), get_peer_id(2));
    assert!(matches!(result, Err(CommitteeStoreError::StakerAlreadyMapped(_))));
}

#[test]
fn staker_ref_count_tracks_across_epochs() {
    let mut store = CommitteeStore::new(2);

    // Staker 1 is in epochs 1 and 2.
    store.add_committee(1, committee_id(1), vec![member(1)]).unwrap();
    store.add_committee(2, committee_id(2), vec![member(1)]).unwrap();

    store.add_peer_for_staker(staker(1), get_peer_id(1)).unwrap();

    // Add epoch 3 (staker 2 only). Evicts epoch 1. Staker 1 ref count: 2 -> 1 (still in epoch 2).
    let peers = store.add_committee(3, committee_id(3), vec![member(2)]).unwrap();
    assert!(peers.is_empty());

    // Add epoch 4 with staker 1 again. Staker 1 ref count: 1 -> 2 (inserted first), then evict
    // epoch 2: 2 -> 1. Staker 1 is NOT disconnected because the new epoch was inserted before
    // eviction. This is the key scenario that requires insert-before-evict ordering.
    let peers = store.add_committee(4, committee_id(4), vec![member(1)]).unwrap();
    assert!(peers.is_empty()); // staker 1 still in epoch 4
}
