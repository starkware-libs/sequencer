use std::sync::OnceLock;

/// The central marker is the first block number that doesn't exist yet.
pub const APOLLO_CENTRAL_BLOCK_MARKER: &str = "apollo_central_block_marker";

/// The header marker is the first block number for which the node does not have a header.
pub const APOLLO_HEADER_MARKER: &str = "apollo_header_marker";

/// The body marker is the first block number for which the node does not have a body.
pub const APOLLO_BODY_MARKER: &str = "apollo_body_marker";

/// The state marker is the first block number for which the node does not have a state body.
pub const APOLLO_STATE_MARKER: &str = "apollo_state_marker";

/// The compiled class marker is the first block number for which the node does not have all of the
/// corresponding compiled classes.
pub const APOLLO_COMPILED_CLASS_MARKER: &str = "apollo_compiled_class_marker";

/// The base layer marker is the first block number for which the node does not guarantee L1
/// finality.
pub const APOLLO_BASE_LAYER_MARKER: &str = "apollo_base_layer_marker";

/// The latency, in seconds, between a block timestamp (as state in its header) and the time the
/// node stores the header.
pub const APOLLO_HEADER_LATENCY_SEC: &str = "apollo_header_latency";

// TODO(Shahak): consider making this value non static and add a way to change this while the app is
// running. e.g via a monitoring endpoint.
/// Global variable set by the main config to enable collecting profiling metrics.
pub static COLLECT_PROFILING_METRICS: OnceLock<bool> = OnceLock::new();

/// The height consensus is currently working on.
pub const APOLLO_CONSENSUS_HEIGHT: &str = "apollo_consensus_height";

/// The number of times consensus has progressed due to the sync protocol.
pub const APOLLO_CONSENSUS_SYNC_COUNT: &str = "apollo_consensus_sync_count";
