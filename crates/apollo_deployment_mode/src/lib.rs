use serde::{Deserialize, Serialize};

/// Deployment mode for the sequencer node.
/// Determines which features and behaviors are enabled across components.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentMode {
    /// Production mode - standard Starknet behavior.
    #[default]
    Starknet,
    /// Echonet mode - test/replay mode with special features:
    /// - Uses state sync block timestamps instead of system clock
    /// - Uses FIFO transaction queue instead of fee-based priority
    /// - Enables pre-confirmed cende integration
    Echonet,
}
