use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use starknet_sequencer_node::deployment_definitions::{
    MAIN_DEPLOYMENT,
    MAIN_DEPLOYMENT_PRESET_PATH,
};

// TODO(Tsabary): add test that the output is updated

/// Creates the dashboard json file.
fn main() {
    // Create file writer.
    let file = File::create(MAIN_DEPLOYMENT_PRESET_PATH)
        .expect("Should have been able to create the output file.");
    let mut writer = BufWriter::new(file);

    // TODO(Tsabary): align all the dump config/preset binaries and tests to be of the same format.
    // Consider using `dump_json_data`, or a variant of it.

    // Add config as JSON content to writer.
    serde_json::to_writer_pretty(&mut writer, &MAIN_DEPLOYMENT)
        .expect("Should have been able to serialize the dashboard to JSON");

    // Add an extra newline after the JSON content.
    writer.write_all(b"\n").expect("Should have been able to write the newline to the file.");

    // Write to file.
    writer.flush().expect("Should have been able to flush the writer.");

    assert!(
        PathBuf::from(&MAIN_DEPLOYMENT_PRESET_PATH).exists(),
        "Failed generating sequencer dashboard data file: {:?}",
        MAIN_DEPLOYMENT_PRESET_PATH
    );

    println!("Generated sequencer dashboard data file: {:?}", MAIN_DEPLOYMENT_PRESET_PATH);
}
