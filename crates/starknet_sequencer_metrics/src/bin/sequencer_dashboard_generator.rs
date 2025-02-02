use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use clap::{Arg, Command};
use starknet_sequencer_metrics::dashboard_definitions::SEQUENCER_DASHBOARD;

/// Creates the dashboard json file.
fn main() {
    // Parse the command line arguments.
    let matches = Command::new("sequencer_dashboard_generator")
        .arg(Arg::new("output").short('o').long("output").help("The output file path"))
        .get_matches();

    let output_file_path: PathBuf = matches
        .get_one::<String>("output")
        .expect("Should have received an output file location arg.")
        .into();

    // Serialize the dashboard to a JSON string.
    let json_string = serde_json::to_string_pretty(&SEQUENCER_DASHBOARD)
        .expect("Should have been able to serialize the dashboard to JSON");

    // Write the JSON string to a file.
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&output_file_path)
        .expect("Should have been able to open the output file.");
    file.write_all(json_string.as_bytes())
        .expect("Should have been able to write the JSON string to the file.");

    // Add an extra newline after the JSON content.
    file.write_all(b"\n").expect("Should have been able to write the newline to the file.");

    assert!(
        PathBuf::from(&output_file_path).exists(),
        "Failed generating sequencer dashboard data file: {:?}",
        output_file_path
    );
}
