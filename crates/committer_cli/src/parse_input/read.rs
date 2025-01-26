use std::fs::File;
use std::io::BufWriter;

use serde::{Deserialize, Serialize};
use starknet_patricia::storage::errors::DeserializationError;
use tracing::info;

use crate::parse_input::cast::InputImpl;
use crate::parse_input::raw_input::RawInput;

#[cfg(test)]
#[path = "read_test.rs"]
pub mod read_test;

type DeserializationResult<T> = Result<T, DeserializationError>;

pub fn parse_input(input: &str) -> DeserializationResult<InputImpl> {
    serde_json::from_str::<RawInput>(input)?.try_into()
}

pub fn read_input(input_path: String) -> String {
    String::from_utf8(
        std::fs::read(input_path.clone())
            .unwrap_or_else(|_| panic!("Failed to read from {input_path}")),
    )
    .expect("Failed to convert bytes to string.")
}

pub fn load_input<T: for<'a> Deserialize<'a>>(input_path: String) -> T {
    info!("Reading input from file: {input_path}.");
    let input_bytes = std::fs::read(input_path.clone())
        .unwrap_or_else(|_| panic!("Failed to read from {input_path}"));
    info!("Done reading {} bytes from {input_path}. Deserializing...", input_bytes.len());
    let result = serde_json::from_slice::<T>(&input_bytes)
        .unwrap_or_else(|_| panic!("Failed to deserialize data from {input_path}"));
    info!("Successfully deserialized data from {input_path}.");
    result
}

pub fn write_to_file<T: Serialize>(file_path: &str, object: &T) {
    let file_buffer = BufWriter::new(File::create(file_path).expect("Failed to create file"));
    serde_json::to_writer(file_buffer, object).expect("Failed to serialize");
}
