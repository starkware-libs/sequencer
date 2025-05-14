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
