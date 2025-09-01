//! Peer reputation and misconduct scoring system.
//!
//! This module implements a reputation system for tracking peer behavior and identifying
//! malicious actors in the network. The scoring system uses a normalized scale where
//! peers accumulate misconduct points based on their behavior.
//!
//! ## Scoring System
//!
//! - **Range**: [0.0, 1.0] where 0.0 is perfectly well-behaved and 1.0 is malicious
//! - **Accumulation**: Scores add up over time as misconduct is detected
//! - **Threshold**: Peers reaching a score of 1.0 are considered malicious
//! - **Actions**: Malicious peers may be disconnected or ignored
//!
//! ## Usage Examples
//!
//! ```rust
//! use apollo_network::misconduct_score::MisconductScore;
//!
//! // Start with neutral reputation
//! let mut peer_score = MisconductScore::NEUTRAL;
//!
//! // Add misconduct for protocol violations
//! peer_score += MisconductScore::new(0.3); // Minor violation
//! peer_score += MisconductScore::new(0.8); // Major violation
//!
//! // Check if peer is now malicious
//! if peer_score.is_malicious() {
//!     println!("Peer should be disconnected");
//! }
//! ```

use std::ops::AddAssign;

/// MisconductScore is in the range [0, 1].
///
/// When a peer's total MisconductScore reaches 1, it is considered malicious.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct MisconductScore {
    score: f64,
}

impl MisconductScore {
    pub const MALICIOUS: MisconductScore = Self { score: 1.0 };
    pub const NEUTRAL: MisconductScore = Self { score: 0.0 };

    pub fn new(score: f64) -> Self {
        assert!(Self::NEUTRAL.score <= score);
        assert!(score <= Self::MALICIOUS.score);
        Self { score }
    }

    pub fn is_malicious(&self) -> bool {
        &Self::MALICIOUS <= self
    }
}

impl AddAssign for MisconductScore {
    fn add_assign(&mut self, rhs: Self) {
        self.score += rhs.score;
        if *self > Self::MALICIOUS {
            *self = Self::MALICIOUS;
        }
    }
}
