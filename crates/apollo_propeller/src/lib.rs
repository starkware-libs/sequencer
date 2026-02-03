pub mod behaviour;
pub mod config;
pub mod engine;
pub mod handler;
pub mod merkle;
pub mod message_processor;
pub mod metrics;
pub mod padding;
mod protocol;
// TODO(AndrewL): Consider renaming this to `erasure_coding` or `error_correction_code`.
pub mod reed_solomon;
pub mod sharding;
pub mod signature;
pub mod time_cache;
// TODO(AndrewL): rename file
pub mod tree;
pub mod types;
pub mod unit;
pub mod unit_validator;

#[cfg(test)]
mod behaviour_test;
#[cfg(test)]
mod merkle_test;
#[cfg(test)]
mod padding_test;
#[cfg(test)]
mod reed_solomon_test;
#[cfg(test)]
mod sharding_test;
#[cfg(test)]
mod signature_test;
#[cfg(test)]
mod time_cache_test;
#[cfg(test)]
mod tree_test;
#[cfg(test)]
mod unit_validator_test;

pub use behaviour::Behaviour;
pub use config::Config;
pub use handler::{Handler, HandlerIn, HandlerOut};
pub use merkle::{MerkleHash, MerkleProof, MerkleTree};
pub use metrics::PropellerMetrics;
pub use tree::{PropellerScheduleManager, Stake};
pub use types::{
    Channel,
    Event,
    MessageRoot,
    PeerSetError,
    ReconstructionError,
    ShardIndex,
    ShardPublishError,
    ShardValidationError,
    TreeGenerationError,
};
pub use unit::PropellerUnit;
pub use unit_validator::UnitValidator;

// TODO(AndrewL): Make tests in this crate have deterministic random peer IDs.
