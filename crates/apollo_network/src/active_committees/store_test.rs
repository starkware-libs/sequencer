use std::collections::HashSet;

use apollo_network_types::test_utils::get_peer_id;
use rstest::{fixture, rstest};
use starknet_api::staking::StakingWeight;

use super::ActiveCommittees;
use crate::active_committees::types::CommitteeMember;

fn member(index: u8) -> CommitteeMember {
    CommitteeMember { peer_id: get_peer_id(index), weight: StakingWeight(1) }
}

fn member_with_weight(index: u8, weight: u128) -> CommitteeMember {
    CommitteeMember { peer_id: get_peer_id(index), weight: StakingWeight(weight) }
}

fn peer_ids(indices: &[u8]) -> HashSet<libp2p::PeerId> {
    indices.iter().map(|&i| get_peer_id(i)).collect()
}

#[fixture]
fn store() -> ActiveCommittees {
    ActiveCommittees::new(3)
}

#[rstest]
fn register_single_epoch(mut store: ActiveCommittees) {
    let result = store.register_epoch(1, vec![member(1), member(2)]);

    assert!(result.new_committee.is_some());
    assert!(result.removed_committee.is_none());
    assert_eq!(result.allowed_peers, peer_ids(&[1, 2]));
}

#[rstest]
fn register_returns_peers_and_weights(mut store: ActiveCommittees) {
    let result =
        store.register_epoch(1, vec![member_with_weight(1, 10), member_with_weight(2, 20)]);

    let (_, committee_peers) = result.new_committee.unwrap();
    let peer_set: HashSet<_> = committee_peers.iter().map(|(p, _)| *p).collect();
    assert_eq!(peer_set, peer_ids(&[1, 2]));
    let weight_sum: u128 = committee_peers.iter().map(|(_, w)| w.0).sum();
    assert_eq!(weight_sum, 30);
}

#[rstest]
fn same_members_reuse_committee(mut store: ActiveCommittees) {
    store.register_epoch(1, vec![member(1), member(2)]);
    let result = store.register_epoch(2, vec![member(1), member(2)]);

    assert!(result.new_committee.is_none());
    assert_eq!(result.allowed_peers, peer_ids(&[1, 2]));
}

#[rstest]
fn member_order_does_not_affect_committee_id() {
    let mut store = ActiveCommittees::new(2);
    let result1 = store.register_epoch(1, vec![member(1), member(2)]);
    let result2 = store.register_epoch(2, vec![member(2), member(1)]);

    let (id1, _) = result1.new_committee.unwrap();
    assert!(result2.new_committee.is_none());

    // Both epochs reference the same committee. Need to evict both to remove it.
    store.register_epoch(3, vec![member(3)]); // evicts epoch 1, committee still has epoch 2
    let result4 = store.register_epoch(4, vec![member(4)]); // evicts epoch 2
    assert_eq!(result4.removed_committee, Some(id1));
}

#[rstest]
fn different_weights_produce_different_committee(mut store: ActiveCommittees) {
    let result1 = store.register_epoch(1, vec![member_with_weight(1, 10)]);
    let result2 = store.register_epoch(2, vec![member_with_weight(1, 20)]);

    let (id1, _) = result1.new_committee.unwrap();
    let (id2, _) = result2.new_committee.unwrap();
    assert_ne!(id1, id2);
}

#[rstest]
#[case::basic(2, vec![
    vec![member(1)],
    vec![member(2)],
    vec![member(3)],
], Some(0), &[2, 3])]
#[case::committee_still_referenced(2, vec![
    vec![member(1)],
    vec![member(1)],  // same committee, ref count = 2
    vec![member(2)],  // evicts epoch 1, but committee still referenced by epoch 2
], None, &[1, 2])]
#[case::immediate_eviction(1, vec![
    vec![member(1)],
    vec![member(2)],
], Some(0), &[2])]
fn eviction(
    #[case] capacity: usize,
    #[case] epoch_members: Vec<Vec<CommitteeMember>>,
    #[case] expect_removed_matches_epoch: Option<usize>,
    #[case] expected_allowed: &[u8],
) {
    let mut store = ActiveCommittees::new(capacity);
    let mut results = Vec::new();
    for (epoch_idx, members) in epoch_members.into_iter().enumerate() {
        results.push(store.register_epoch(u64::try_from(epoch_idx).unwrap() + 1, members));
    }

    let last = results.last().unwrap();
    match expect_removed_matches_epoch {
        Some(epoch_idx) => {
            let (expected_id, _) = results[epoch_idx].new_committee.as_ref().unwrap();
            assert_eq!(last.removed_committee, Some(*expected_id));
        }
        None => {
            // Either no removal, or the removed committee is NOT the one from the
            // still-referenced epoch.
            if let Some(removed) = last.removed_committee {
                let (still_ref_id, _) = results[0].new_committee.as_ref().unwrap();
                assert_ne!(removed, *still_ref_id);
            }
        }
    }
    assert_eq!(last.allowed_peers, peer_ids(expected_allowed));
}

#[rstest]
fn allowed_peers_union_of_all_committees(mut store: ActiveCommittees) {
    store.register_epoch(1, vec![member(1), member(2)]);
    let result = store.register_epoch(2, vec![member(2), member(3)]);

    assert_eq!(result.allowed_peers, peer_ids(&[1, 2, 3]));
}

#[rstest]
fn peer_in_multiple_committees_stays_allowed_until_all_evicted(mut store: ActiveCommittees) {
    // Peer 1 is in two different committees.
    store.register_epoch(1, vec![member(1), member(2)]);
    store.register_epoch(2, vec![member(1), member(3)]);
    store.register_epoch(3, vec![member(4)]);

    // Evict epoch 1. Peer 1 still in epoch 2's committee.
    let result = store.register_epoch(4, vec![member(5)]);
    assert!(result.allowed_peers.contains(&get_peer_id(1)));

    // Evict epoch 2. Peer 1 no longer in any committee.
    let result = store.register_epoch(5, vec![member(6)]);
    assert!(!result.allowed_peers.contains(&get_peer_id(1)));
}

#[rstest]
fn empty_committee(mut store: ActiveCommittees) {
    let result = store.register_epoch(1, vec![]);

    let (_, propeller_peers) = result.new_committee.unwrap();
    assert!(propeller_peers.is_empty());
    assert!(result.allowed_peers.is_empty());
}

#[rstest]
fn committee_reappears_after_eviction() {
    let mut store = ActiveCommittees::new(2);

    let result1 = store.register_epoch(1, vec![member(1)]);
    let (original_id, _) = result1.new_committee.unwrap();

    store.register_epoch(2, vec![member(2)]);
    let result3 = store.register_epoch(3, vec![member(3)]); // evicts epoch 1
    assert_eq!(result3.removed_committee, Some(original_id));

    // Re-register the same committee. Should be treated as new.
    let result4 = store.register_epoch(4, vec![member(1)]);
    let (reappeared_id, _) = result4.new_committee.unwrap();
    assert_eq!(reappeared_id, original_id);
}

#[rstest]
fn gradual_eviction_of_multi_referenced_committee(mut store: ActiveCommittees) {
    let result1 = store.register_epoch(1, vec![member(1)]);
    let (committee_id, _) = result1.new_committee.unwrap();

    // All three epochs reference the same committee.
    store.register_epoch(2, vec![member(1)]);
    store.register_epoch(3, vec![member(1)]);

    // Evict epochs one by one. Committee survives until all refs gone.
    let result4 = store.register_epoch(4, vec![member(2)]); // evicts epoch 1
    assert_ne!(result4.removed_committee, Some(committee_id));
    assert!(result4.allowed_peers.contains(&get_peer_id(1)));

    let result5 = store.register_epoch(5, vec![member(3)]); // evicts epoch 2
    assert_ne!(result5.removed_committee, Some(committee_id));
    assert!(result5.allowed_peers.contains(&get_peer_id(1)));

    let result6 = store.register_epoch(6, vec![member(4)]); // evicts epoch 3
    assert_eq!(result6.removed_committee, Some(committee_id));
    assert!(!result6.allowed_peers.contains(&get_peer_id(1)));
}
