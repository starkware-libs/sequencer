use std::env;
use std::path::Path;

/// Main entry point of the committer CLI.
fn main() {
    // Open the input file.
    let args: Vec<String> = env::args().collect();
    let input_file_name = Path::new(&args[1]);
    let output_file_name = Path::new(&args[2]);
    assert!(
        input_file_name.is_absolute() && output_file_name.is_absolute(),
        "Given paths must be absolute"
    );

    // Business logic to be implemented here.
    let output = std::fs::read(input_file_name).unwrap();

    // Output to file.
    std::fs::write(output_file_name, output).expect("Failed to write output");
}
