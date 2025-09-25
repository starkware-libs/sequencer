use serde::{Deserialize, Serialize};
use thiserror::Error;

// TODO(Nadin/Tsabary): Add more errors, and return the errors from the config manager.
#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigManagerError {
    #[error("Config file not found: {0}")]
    ConfigNotFound(String),
}
