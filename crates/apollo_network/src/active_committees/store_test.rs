use std::collections::BTreeSet;

use apollo_network_types::test_utils::get_peer_id;
use assert_matches::assert_matches;
use expect_test::expect;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::staking::StakingWeight;

use super::{ActiveCommittees, AddEpochError};
use crate::active_committees::types::CommitteeMember;

fn member(index: u8) -> CommitteeMember {
    CommitteeMember { peer_id: get_peer_id(index), weight: StakingWeight(1) }
}

fn member_with_weight(index: u8, weight: u128) -> CommitteeMember {
    CommitteeMember { peer_id: get_peer_id(index), weight: StakingWeight(weight) }
}

fn peer_ids(indices: &[u8]) -> BTreeSet<libp2p::PeerId> {
    indices.iter().map(|&i| get_peer_id(i)).collect()
}

#[fixture]
fn active_committees() -> ActiveCommittees {
    ActiveCommittees::new(3)
}

#[rstest]
fn add_single_epoch(mut active_committees: ActiveCommittees) {
    let output = active_committees.add_epoch(1, vec![member(1), member(2)]).unwrap();

    assert!(output.new_committee.is_some());
    assert!(output.removed_committee.is_none());
    assert_eq!(output.allowed_peers, peer_ids(&[1, 2]));
}

#[rstest]
fn add_epoch_returns_peers_and_weights(mut active_committees: ActiveCommittees) {
    let output = active_committees
        .add_epoch(1, vec![member_with_weight(1, 10), member_with_weight(2, 20)])
        .unwrap();

    let (_, committee_peers) = output.new_committee.unwrap();
    // Members are sorted by peer_id; peer_id ordering is deterministic from get_peer_id.
    let mut expected =
        vec![(get_peer_id(1), StakingWeight(10)), (get_peer_id(2), StakingWeight(20))];
    expected.sort_by_key(|(p, _)| *p);
    let mut actual = committee_peers;
    actual.sort_by_key(|(p, _)| *p);
    assert_eq!(actual, expected);
}

#[rstest]
fn add_existing_committee_returns_none_for_new_committee(mut active_committees: ActiveCommittees) {
    active_committees.add_epoch(1, vec![member(1), member(2)]).unwrap();
    let output = active_committees.add_epoch(2, vec![member(1), member(2)]).unwrap();

    assert!(output.new_committee.is_none());
    assert_eq!(output.allowed_peers, peer_ids(&[1, 2]));
}

#[rstest]
fn member_order_does_not_affect_committee_id() {
    let mut active_committees = ActiveCommittees::new(2);
    let output1 = active_committees.add_epoch(1, vec![member(1), member(2)]).unwrap();
    let output2 = active_committees.add_epoch(2, vec![member(2), member(1)]).unwrap();

    let (id1, _) = output1.new_committee.unwrap();
    assert!(output2.new_committee.is_none());

    // Both epochs reference the same committee. Need to evict both to remove it.
    active_committees.add_epoch(3, vec![member(3)]).unwrap(); // evicts epoch 1
    let output4 = active_committees.add_epoch(4, vec![member(4)]).unwrap(); // evicts epoch 2
    assert_eq!(output4.removed_committee, Some(id1));
}

#[rstest]
fn different_weights_produce_different_committee(mut active_committees: ActiveCommittees) {
    let output1 = active_committees.add_epoch(1, vec![member_with_weight(1, 10)]).unwrap();
    let output2 = active_committees.add_epoch(2, vec![member_with_weight(1, 20)]).unwrap();

    let (id1, _) = output1.new_committee.unwrap();
    let (id2, _) = output2.new_committee.unwrap();
    assert_ne!(id1, id2);
}

// Each case adds epochs sequentially; the last add_epoch call triggers eviction.
#[rstest]
#[case::basic(
    2,
    vec![
        vec![member(1)],
        vec![member(2)],
        vec![member(3)],
    ],
    Some(0),
    &[2, 3],
)]
#[case::committee_still_referenced(
    2,
    vec![
        vec![member(1)],
        vec![member(1)],  // same committee, ref count = 2
        vec![member(2)],  // evicts epoch 0, but committee still referenced by epoch 1
    ],
    None,
    &[1, 2],
)]
#[case::immediate_eviction(
    1,
    vec![
        vec![member(1)],
        vec![member(2)],
    ],
    Some(0),
    &[2],
)]
fn eviction(
    #[case] capacity: usize,
    #[case] epoch_members: Vec<Vec<CommitteeMember>>,
    #[case] epoch_of_removed_committee: Option<usize>,
    #[case] expected_allowed: &[u8],
) {
    let mut active_committees = ActiveCommittees::new(capacity);
    let mut outputs = Vec::new();
    for (epoch_idx, members) in epoch_members.into_iter().enumerate() {
        let epoch_id = u64::try_from(epoch_idx).unwrap();
        outputs.push(active_committees.add_epoch(epoch_id, members).unwrap());
    }

    let last_output = outputs.last().unwrap();
    match epoch_of_removed_committee {
        Some(epoch_idx) => {
            let (expected_id, _) = outputs[epoch_idx].new_committee.as_ref().unwrap();
            assert_eq!(last_output.removed_committee, Some(*expected_id));
        }
        None => {
            assert!(last_output.removed_committee.is_none());
        }
    }
    assert_eq!(last_output.allowed_peers, peer_ids(expected_allowed));
}

#[rstest]
fn allowed_peers_union_of_all_committees(mut active_committees: ActiveCommittees) {
    active_committees.add_epoch(1, vec![member(1), member(2)]).unwrap();
    let output = active_committees.add_epoch(2, vec![member(2), member(3)]).unwrap();

    assert_eq!(output.allowed_peers, peer_ids(&[1, 2, 3]));
}

#[rstest]
fn peer_in_multiple_committees_stays_allowed_until_all_evicted(
    mut active_committees: ActiveCommittees,
) {
    // Peer 1 is in two different committees.
    active_committees.add_epoch(1, vec![member(1), member(2)]).unwrap();
    active_committees.add_epoch(2, vec![member(1), member(3)]).unwrap();
    active_committees.add_epoch(3, vec![member(4)]).unwrap();

    // Evict epoch 1. Peer 1 still in epoch 2's committee.
    let output = active_committees.add_epoch(4, vec![member(5)]).unwrap();
    assert!(output.allowed_peers.contains(&get_peer_id(1)));

    // Evict epoch 2. Peer 1 no longer in any committee.
    let output = active_committees.add_epoch(5, vec![member(6)]).unwrap();
    assert!(!output.allowed_peers.contains(&get_peer_id(1)));
}

#[rstest]
fn empty_committee(mut active_committees: ActiveCommittees) {
    let output = active_committees.add_epoch(1, vec![]).unwrap();

    let (_, peers) = output.new_committee.unwrap();
    assert!(peers.is_empty());
    assert!(output.allowed_peers.is_empty());
}

#[rstest]
fn committee_reappears_after_eviction() {
    let mut active_committees = ActiveCommittees::new(2);

    let output1 = active_committees.add_epoch(1, vec![member(1)]).unwrap();
    let (original_id, _) = output1.new_committee.unwrap();

    active_committees.add_epoch(2, vec![member(2)]).unwrap();
    let output3 = active_committees.add_epoch(3, vec![member(3)]).unwrap(); // evicts epoch 1
    assert_eq!(output3.removed_committee, Some(original_id));

    // Re-add the same committee. Should be treated as new.
    let output4 = active_committees.add_epoch(4, vec![member(1)]).unwrap();
    let (reappeared_id, _) = output4.new_committee.unwrap();
    assert_eq!(reappeared_id, original_id);
}

#[rstest]
fn gradual_eviction_of_multi_referenced_committee(mut active_committees: ActiveCommittees) {
    let output1 = active_committees.add_epoch(1, vec![member(1)]).unwrap();
    let (committee_id, _) = output1.new_committee.unwrap();

    // All three epochs reference the same committee.
    active_committees.add_epoch(2, vec![member(1)]).unwrap();
    active_committees.add_epoch(3, vec![member(1)]).unwrap();

    // Evict epochs one by one. Committee survives until all refs gone.
    let output4 = active_committees.add_epoch(4, vec![member(2)]).unwrap(); // evicts epoch 1
    assert_ne!(output4.removed_committee, Some(committee_id));
    assert!(output4.allowed_peers.contains(&get_peer_id(1)));

    let output5 = active_committees.add_epoch(5, vec![member(3)]).unwrap(); // evicts epoch 2
    assert_ne!(output5.removed_committee, Some(committee_id));
    assert!(output5.allowed_peers.contains(&get_peer_id(1)));

    let output6 = active_committees.add_epoch(6, vec![member(4)]).unwrap(); // evicts epoch 3
    assert_eq!(output6.removed_committee, Some(committee_id));
    assert!(!output6.allowed_peers.contains(&get_peer_id(1)));
}

#[rstest]
fn duplicate_epoch_id_returns_error(mut active_committees: ActiveCommittees) {
    active_committees.add_epoch(1, vec![member(1)]).unwrap();
    let result = active_committees.add_epoch(1, vec![member(2)]);
    assert_matches!(result, Err(AddEpochError::DuplicateEpochId(1)));
}

#[rstest]
fn duplicate_peer_id_returns_error(mut active_committees: ActiveCommittees) {
    let result = active_committees.add_epoch(1, vec![member(1), member(1)]);
    assert_matches!(result, Err(AddEpochError::DuplicatePeerId(_)));
}

#[rstest]
fn committee_id_hash_regression(mut active_committees: ActiveCommittees) {
    let output = active_committees.add_epoch(1, vec![member(1), member(2)]).unwrap();
    let (committee_id, _) = output.new_committee.unwrap();

    let hex_str: String = committee_id.0.iter().map(|b| format!("{b:02x}")).collect();
    expect!["18b2a63f07152bd0767481dd927c371994aa0dccac21957783c95047ce231557"].assert_eq(&hex_str);
}
