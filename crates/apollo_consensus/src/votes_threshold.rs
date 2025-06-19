use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "votes_threshold_test.rs"]
mod votes_threshold_test;

/// Represents a threshold for the number of votes (out of total votes) required to meet a quorum.
/// For example, a threshold of 2/3 means that more than 2/3 of the total votes must be in favor.
/// Note that if the number of votes is exactly equal to the denominator, the threshold is not met.
/// If the total number of votes is zero, the threshold is not met.
#[derive(Serialize, Deserialize)]
pub struct VotesThreshold {
    numerator: u64,
    denominator: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum QuorumType {
    #[default]
    Byzantine,
    Honest,
}

// Standard Tendermint consensus threshold.
pub const BYZANTINE_QUORUM: VotesThreshold = VotesThreshold::new(2, 3);
pub const ROUND_SKIP_THRESHOLD: VotesThreshold = VotesThreshold::new(1, 3);

// Assumes no malicious validators.
pub const HONEST_QUORUM: VotesThreshold = VotesThreshold::new(1, 2);

impl VotesThreshold {
    const fn new(numerator: u64, denominator: u64) -> Self {
        assert!(denominator > 0, "Denominator must be greater than zero");
        assert!(denominator >= numerator, "Denominator must be greater than or equal to numerator");
        Self { numerator, denominator }
    }

    pub fn from_quorum_type(quorum_type: QuorumType) -> Self {
        match quorum_type {
            QuorumType::Byzantine => BYZANTINE_QUORUM,
            QuorumType::Honest => HONEST_QUORUM,
        }
    }

    pub fn is_met(&self, amount: u64, total: u64) -> bool {
        amount.checked_mul(self.denominator).expect("Numeric overflow")
            > total.checked_mul(self.numerator).expect("Numeric overflow")
    }
}
