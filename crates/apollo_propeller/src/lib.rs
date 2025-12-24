pub mod behaviour;
pub mod config;
pub mod handler;
pub mod merkle;
#[cfg(test)]
mod merkle_test;
pub mod protocol;
// TODO(AndrewL): Consider renaming this to `erasure_coding` or `error_correction_code`.
pub mod reed_solomon;
#[cfg(test)]
mod reed_solomon_test;
pub mod signature;
#[cfg(test)]
mod signature_test;
// TODO(AndrewL): rename file
pub mod tree;
#[cfg(test)]
mod tree_test;
pub mod types;
pub mod unit;
pub mod unit_validator;
#[cfg(test)]
mod unit_validator_test;

pub use behaviour::Behaviour;
pub use config::Config;
pub use handler::{Handler, HandlerIn, HandlerOut};
pub use merkle::{MerkleHash, MerkleProof, MerkleTree};
pub use tree::{PropellerScheduleManager, Stake};
pub use types::{Channel, Event, MessageRoot, ShardIndex, ShardValidationError};
pub use unit::PropellerUnit;
pub use unit_validator::UnitValidator;

// TODO(AndrewL): Make tests in this crate have deterministic random peer IDs.
