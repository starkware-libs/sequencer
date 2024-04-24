use crate::deserialization::types::Input;
use crate::deserialization::types::RawInput;
use crate::storage::errors::DeserializationError;

#[cfg(test)]
#[path = "read_test.rs"]
pub mod read_test;

type DeserializationResult<T> = Result<T, DeserializationError>;

#[allow(dead_code)]
pub(crate) fn parse_input(input: String) -> DeserializationResult<Input> {
    serde_json::from_str::<RawInput>(&input)?.try_into()
}
