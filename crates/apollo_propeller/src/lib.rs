pub mod merkle;
#[cfg(test)]
mod merkle_test;
// TODO(AndrewL): Consider renaming this to `erasure_coding` or `error_correction_code`.
pub mod reed_solomon;
pub mod signature;
#[cfg(test)]
mod signature_test;
pub mod types;

pub use merkle::MerkleHash;
pub use types::{Channel, MessageRoot, ShardIndex};
