#[cfg(any(feature = "testing", test))]
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[cfg(any(feature = "testing", test))]
use colored::Colorize;
use serde::Serialize;

#[cfg(any(feature = "testing", test))]
use crate::path::resolve_project_relative_path;
#[cfg(any(feature = "testing", test))]
use crate::test_utils::assert_json_eq;

#[cfg(any(feature = "testing", test))]
pub fn serialize_to_file_test<T: Serialize>(data: T, file_path: &str) {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    let loaded_data: serde_json::Value =
        serde_json::from_reader(File::open(file_path).unwrap()).unwrap();

    let serialized_data =
        serde_json::to_value(&data).expect("Should have been able to serialize the data to JSON");

    let error_message = format!(
        "{}\n{}",
        "Dump file doesn't match the data. Please update it using the binary.".purple().bold(),
        "Diffs shown below (loaded file <<>> data serialization)."
    );
    assert_json_eq(&loaded_data, &serialized_data, error_message);
}

pub fn serialize_to_file<T: Serialize>(data: T, file_path: &str) {
    // Create file writer.
    let file = File::create(file_path)
        .unwrap_or_else(|_| panic!("Failed generating data file: {:?}", file_path));

    let mut writer = BufWriter::new(file);

    // Add config as JSON content to writer.
    serde_json::to_writer_pretty(&mut writer, &data)
        .expect("Should have been able to serialize input data to JSON.");

    // Add an extra newline after the JSON content.
    writer.write_all(b"\n").expect("Should have been able to write the newline to the file.");

    // Write to file.
    writer.flush().expect("Should have been able to flush the writer.");

    assert!(PathBuf::from(&file_path).exists(), "Failed generating data file: {:?}", file_path);

    println!("Generated data file: {:?}", file_path);
}
