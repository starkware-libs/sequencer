use std::fs::File;
use std::io::{self, BufWriter};

use serde::{Deserialize, Serialize};
use starknet_patricia::storage::errors::DeserializationError;

use crate::parse_input::cast::InputImpl;
use crate::parse_input::raw_input::RawInput;

#[cfg(test)]
#[path = "read_test.rs"]
pub mod read_test;

type DeserializationResult<T> = Result<T, DeserializationError>;

pub fn parse_input(input: &str) -> DeserializationResult<InputImpl> {
    serde_json::from_str::<RawInput>(input)?.try_into()
}

pub fn read_from_stdin() -> String {
    io::read_to_string(io::stdin()).expect("Failed to read from stdin.")
}

pub fn load_from_stdin<T: for<'a> Deserialize<'a>>() -> T {
    let stdin = read_from_stdin();
    serde_json::from_str(&stdin).expect("Failed to load from stdin")
}

pub fn write_to_file<T: Serialize>(file_path: &str, object: &T) {
    let file_buffer = BufWriter::new(File::create(file_path).expect("Failed to create file"));
    serde_json::to_writer(file_buffer, object).expect("Failed to serialize");
}
