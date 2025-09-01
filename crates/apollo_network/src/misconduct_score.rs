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

/// Represents a peer's misconduct score in the range [0.0, 1.0].
///
/// The misconduct score is used to track peer reputation and identify malicious
/// behavior. Scores accumulate over time as misconduct is detected, and peers
/// reaching the maximum score are considered malicious.
///
/// # Score Ranges
///
/// - **0.0**: Perfectly well-behaved peer (neutral)
/// - **0.1-0.9**: Varying degrees of misconduct
/// - **1.0**: Malicious peer (should be disconnected)
///
/// # Thread Safety
///
/// This type is `Copy` and thread-safe, making it suitable for use across
/// multiple threads without synchronization overhead.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct MisconductScore {
    score: f64,
}

impl MisconductScore {
    /// Maximum misconduct score indicating a malicious peer.
    ///
    /// Peers with this score should be disconnected and potentially blocked
    /// from future connections.
    ///
    /// ```
    /// # use apollo_network::misconduct_score::MisconductScore;
    /// assert!(MisconductScore::MALICIOUS.is_malicious());
    /// ```
    pub const MALICIOUS: MisconductScore = Self { score: 1.0 };

    /// Neutral misconduct score for well-behaved peers.
    ///
    /// This is the starting score for new peers with no history of misconduct.
    ///
    /// ```
    /// # use apollo_network::misconduct_score::MisconductScore;
    /// assert!(!MisconductScore::NEUTRAL.is_malicious());
    /// ```
    pub const NEUTRAL: MisconductScore = Self { score: 0.0 };

    /// Creates a new misconduct score with the specified value.
    ///
    /// # Arguments
    ///
    /// * `score` - The misconduct score value, must be in the range [0.0, 1.0]
    ///
    /// # Returns
    ///
    /// A new `MisconductScore` instance with the specified score.
    ///
    /// # Panics
    ///
    /// Panics if the score is outside the valid range [0.0, 1.0].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use apollo_network::misconduct_score::MisconductScore;
    ///
    /// let minor_violation = MisconductScore::new(0.2);
    /// let major_violation = MisconductScore::new(0.8);
    ///
    /// // This would panic:
    /// // let invalid = MisconductScore::new(1.5);
    /// ```
    pub fn new(score: f64) -> Self {
        assert!(Self::NEUTRAL.score <= score);
        assert!(score <= Self::MALICIOUS.score);
        Self { score }
    }

    /// Checks if this peer should be considered malicious.
    ///
    /// A peer is considered malicious if their misconduct score has reached
    /// the maximum threshold, indicating they should be disconnected and
    /// potentially blocked.
    ///
    /// # Returns
    ///
    /// `true` if the peer is malicious (score >= 1.0), `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use apollo_network::misconduct_score::MisconductScore;
    ///
    /// let neutral_peer = MisconductScore::NEUTRAL;
    /// assert!(!neutral_peer.is_malicious());
    ///
    /// let malicious_peer = MisconductScore::MALICIOUS;
    /// assert!(malicious_peer.is_malicious());
    ///
    /// let minor_violation = MisconductScore::new(0.3);
    /// assert!(!minor_violation.is_malicious());
    /// ```
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
