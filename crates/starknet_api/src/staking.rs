use serde::{Deserialize, Serialize};

/// Epoch identifier, matching the staking contract's epoch.
pub type EpochId = u64;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct StakingWeight(pub u128);
