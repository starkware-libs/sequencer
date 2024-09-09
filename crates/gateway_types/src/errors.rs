use serde::{Deserialize, Serialize};
use thiserror::Error;

// TODO(Tsabary/Shahak): Populate with actual errors.
#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayError {
    #[error("Placeholder error message")]
    Placeholder,
}
