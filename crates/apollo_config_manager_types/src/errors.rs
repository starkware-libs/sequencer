use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigManagerError {
    // TODO(Nadin): Add specific error variants as needed
    #[allow(dead_code)]
    Placeholder,
}
