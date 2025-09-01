use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigManagerError {
    ConfigNotFound(String),
    ConfigParsingError(String),
    ConfigValidationError(String),
    StorageError(String),
}
