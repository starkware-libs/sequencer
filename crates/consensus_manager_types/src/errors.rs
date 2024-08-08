use serde::{Deserialize, Serialize};
use thiserror::Error;

// TODO(Tsabary/Matan): Populate with actual errors.
#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusManagerError {
    #[error("Placeholder error message")]
    Placeholder,
}
