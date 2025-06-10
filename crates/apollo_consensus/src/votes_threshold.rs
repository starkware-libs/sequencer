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

impl VotesThreshold {
    pub fn new(numerator: u64, denominator: u64) -> Self {
        assert!(denominator > 0, "Denominator must be greater than zero");
        assert!(denominator >= numerator, "Denominator must be greater than or equal to numerator");
        Self { numerator, denominator }
    }

    pub fn is_met(&self, amount: u64, total: u64) -> bool {
        amount.checked_mul(self.denominator).expect("Numeric overflow")
            > total.checked_mul(self.numerator).expect("Numeric overflow")
    }
}
