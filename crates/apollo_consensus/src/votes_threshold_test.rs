use crate::votes_threshold::VotesThreshold;

#[test]
#[should_panic]
fn votes_threshold_denominator_zero() {
    let _ = VotesThreshold::new(1, 0);
}

#[test]
#[should_panic]
fn votes_threshold_numerator_greater() {
    // Denominator must be greater than or equal to numerator
    let _ = VotesThreshold::new(2, 1);
}

#[test]
fn votes_threshold_is_met() {
    let threshold = VotesThreshold::new(2, 3);
    assert!(threshold.is_met(3, 4)); // 3 out of 4 votes
    assert!(threshold.is_met(5, 6)); // 5 out of 6 votes
    assert!(threshold.is_met(10, 10)); // All votes in favor

    // Test cases where the threshold is not met
    let threshold = VotesThreshold::new(2, 3);
    assert!(!threshold.is_met(1, 3)); // 1 out of 3 votes
    assert!(!threshold.is_met(2, 3)); // 2 out of 3 votes (not enough, must be above threshold)
    assert!(!threshold.is_met(2, 5)); // 2 out of 5 votes
}

#[test]
fn votes_threshold_amount_required() {
    let threshold = VotesThreshold::new(2, 3);
    assert_eq!(threshold.amount_required(4), 3); // 3 out of 4 votes needed
    assert_eq!(threshold.amount_required(6), 5); // 5 out of 6 votes needed
    assert_eq!(threshold.amount_required(10), 7); // 7 out of 10 votes needed

    // Test cases for amount required with 1/2 threshold
    let threshold = VotesThreshold::new(1, 2);
    assert_eq!(threshold.amount_required(4), 3); // 3 out of 4 votes needed
    assert_eq!(threshold.amount_required(3), 2); // 2 out of 3 votes needed
    assert_eq!(threshold.amount_required(10), 6); // 6 out of 10 votes needed
}
