use std::fs::{create_dir_all, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[cfg(any(feature = "testing", test))]
use colored::Colorize;
use serde::Serialize;
use serde_json::to_writer_pretty;
#[cfg(any(feature = "testing", test))]
use serde_json::{from_reader, to_value, Value};

#[cfg(any(feature = "testing", test))]
use crate::path::resolve_project_relative_path;
#[cfg(any(feature = "testing", test))]
use crate::test_utils::assert_json_eq;

#[cfg(any(feature = "testing", test))]
pub fn serialize_to_file_test<T: Serialize>(data: T, file_path: &str, fix_binary_name: &str) {
    let file_path = resolve_project_relative_path("").unwrap().join(file_path);
    let file = File::open(&file_path).unwrap_or_else(|err| {
        panic!("Failed to open file '{}': {}", file_path.display(), err);
    });
    let loaded_data: Value = from_reader(file).unwrap();
    let serialized_data =
        to_value(&data).expect("Should have been able to serialize the data to JSON");

    let error_message = format!(
        "{}{}{}\n{}",
        "Dump file doesn't match the data, please update it using: 'cargo run --bin "
            .purple()
            .bold(),
        fix_binary_name.purple().bold(),
        "'.".purple().bold(),
        "Diffs shown below (loaded file <<>> data serialization):"
    );
    assert_json_eq(&loaded_data, &serialized_data, error_message);
}

pub fn serialize_to_file<T: Serialize>(data: T, file_path: &str) {
    // Ensure the parent directory exists
    if let Some(parent) = PathBuf::from(file_path).parent() {
        create_dir_all(parent).unwrap_or_else(|err| {
            panic!("Failed to create directory for {file_path}: {err}");
        });
    }

    // Create file writer.
    let file = File::create(file_path)
        .unwrap_or_else(|err| panic!("Failed generating data file: {file_path:?}: {err}"));

    let mut writer = BufWriter::new(file);

    // Add config as JSON content to writer.
    to_writer_pretty(&mut writer, &data)
        .expect("Should have been able to serialize input data to JSON.");

    // Add an extra newline after the JSON content.
    writer.write_all(b"\n").expect("Should have been able to write the newline to the file.");

    // Write to file.
    writer.flush().expect("Should have been able to flush the writer.");

    assert!(PathBuf::from(&file_path).exists(), "Failed generating data file: {file_path:?}");

    println!("Generated data file: {file_path:?}");
}
