use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use starknet_sequencer_metrics::dashboard_definitions::{DEV_JSON_PATH, SEQUENCER_DASHBOARD};

/// Creates the dashboard json file.
fn main() {
    // TODO(Tsabary): add test that the output is updated

    // Create file writer.
    let file =
        File::create(DEV_JSON_PATH).expect("Should have been able to create the output file.");
    let mut writer = BufWriter::new(file);

    // Add config as JSON content to writer.
    serde_json::to_writer_pretty(&mut writer, &SEQUENCER_DASHBOARD)
        .expect("Should have been able to serialize the dashboard to JSON");

    // Add an extra newline after the JSON content.
    writer.write_all(b"\n").expect("Should have been able to write the newline to the file.");

    // Write to file.
    writer.flush().expect("Should have been able to flush the writer.");

    assert!(
        PathBuf::from(&DEV_JSON_PATH).exists(),
        "Failed generating sequencer dashboard data file: {:?}",
        DEV_JSON_PATH
    );

    println!("Generated sequencer dashboard data file: {:?}", DEV_JSON_PATH);
}
