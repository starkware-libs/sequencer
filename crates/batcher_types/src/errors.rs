use serde::{Deserialize, Serialize};
use thiserror::Error;

// TODO(Tsabary/Yael/Dafna): Populate with actual errors.
#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatcherError {
    #[error("Placeholder error message")]
    Placeholder,
}
