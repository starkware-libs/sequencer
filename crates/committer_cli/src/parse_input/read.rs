use std::{fs::File, path::Path};

use committer::{block_committer::input::Input, storage::errors::DeserializationError};
use serde::{Deserialize, Serialize};

use crate::parse_input::raw_input::RawInput;

#[cfg(test)]
#[path = "read_test.rs"]
pub mod read_test;

type DeserializationResult<T> = Result<T, DeserializationError>;

pub fn parse_input(input: String) -> DeserializationResult<Input> {
    serde_json::from_str::<RawInput>(&input)?.try_into()
}

pub fn load_from_file<T: for<'a> Deserialize<'a>>(file_path: &str) -> T {
    let file = File::open(Path::new(file_path)).expect("Failed to open file");
    serde_json::from_reader(&file).expect("Failed to load from file")
}

pub fn write_to_file<T: Serialize>(file_path: &str, object: &T) {
    let file = File::create(file_path).expect("Failed to create file");
    serde_json::to_writer(file, object).expect("Failed to serialize");
}
