use std::path::PathBuf;
use std::process;

use clap::Parser;

#[derive(Parser, Debug)]
#[clap(version, verbatim_doc_comment)]
struct Args {
    /// The path of the Sierra file to compile.
    path: PathBuf,
    /// The output file name (default: stdout).
    output: Option<String>,
}

fn main() {
    let args = Args::parse();
    println!("Path : {:?}", args.path);
    println!("Child PID: {}", process::id());
    // Your task code here
}
