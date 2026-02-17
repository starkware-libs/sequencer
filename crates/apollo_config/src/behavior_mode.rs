use serde::{Deserialize, Serialize};

/// Behavior mode - determines which features and operational behaviors are enabled.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BehaviorMode {
    /// Production mode - standard Starknet behavior.
    #[default]
    Starknet,
    /// Echonet mode - test/replay mode with special features:
    /// - Uses original block timestamps instead of system clock
    /// - Uses FIFO transaction queue instead of fee-based priority
    Echonet,
}
