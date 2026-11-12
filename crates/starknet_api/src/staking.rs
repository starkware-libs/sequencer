use serde::{Deserialize, Serialize};

/// Epoch identifier, matching the staking contract's epoch.
pub type EpochId = u64;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct StakingWeight(pub u128);

/// A temporary constant to use as a validator ID. Zero is not a valid contract address.
// TODO(Matan): Remove this once we have a proper validator set.
pub const DEFAULT_VALIDATOR_ID: u64 = 100;
