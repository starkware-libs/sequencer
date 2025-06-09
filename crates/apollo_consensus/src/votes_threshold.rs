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
    fn new(numerator: u64, denominator: u64) -> Self {
        assert!(denominator > 0, "Denominator must be greater than zero");
        assert!(denominator >= numerator, "Denominator must be greater than or equal to numerator");
        Self { numerator, denominator }
    }

    pub fn from_quorum_type(quorum_type: QuorumType) -> Self {
        match quorum_type {
            QuorumType::Byzantine => Self::from_two_thirds(),
            QuorumType::Honest => Self::from_one_half(),
        }
    }

    pub fn from_skip_round() -> Self {
        // Represents a 1/3 threshold, used for skip round
        Self::from_one_third()
    }

    pub fn from_two_thirds() -> Self {
        // Represents a 2/3 threshold
        Self::new(2, 3)
    }

    pub fn from_one_third() -> Self {
        // Represents a 1/3 threshold
        Self::new(1, 3)
    }

    pub fn from_one_half() -> Self {
        // Represents a 1/2 threshold
        Self::new(1, 2)
    }

    pub fn is_met(&self, amount: u64, total: u64) -> bool {
        if total == 0 {
            return false; // Avoid division by zero
        }
        amount * self.denominator > total * self.numerator
    }
}

pub enum QuorumType {
    Byzantine,
    Honest,
}
