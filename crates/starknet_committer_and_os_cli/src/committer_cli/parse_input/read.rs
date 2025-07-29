use std::collections::HashMap;

use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};

use crate::committer_cli::parse_input::cast::InputImpl;
use crate::committer_cli::parse_input::raw_input::RawInput;

#[cfg(test)]
#[path = "read_test.rs"]
pub mod read_test;

type DeserializationResult<T> = Result<T, DeserializationError>;

pub fn parse_input(input: &str) -> DeserializationResult<(InputImpl, HashMap<DbKey, DbValue>)> {
    serde_json::from_str::<RawInput>(input)?.try_into()
}
