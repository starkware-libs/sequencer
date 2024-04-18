use std::fmt::Debug;

use thiserror::Error;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum DeserializationError {
    #[error("There is a key duplicate at {0} mapping.")]
    KeyDuplicate(String),
    #[error("Couldn't read and parse the given input JSON: {0}")]
    ParsingError(#[from] serde_json::Error),
}
