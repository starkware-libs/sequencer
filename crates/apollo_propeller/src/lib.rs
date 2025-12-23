pub mod merkle;
#[cfg(test)]
mod merkle_test;
// TODO(AndrewL): Consider renaming this to `erasure_coding` or `error_correction_code`.
pub mod reed_solomon;
#[cfg(test)]
mod reed_solomon_test;
pub mod types;

pub use merkle::MerkleHash;
pub use types::{Channel, MessageRoot, ShardIndex};
