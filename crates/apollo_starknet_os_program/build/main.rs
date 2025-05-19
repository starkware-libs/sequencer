use std::path::PathBuf;

mod compile_program;
#[cfg(feature = "dump_source_files")]
mod dump_source;

/// Build script for the `apollo_starknet_os_program` crate.
/// Recompiles the OS program if the source files change.
/// Optionally, also exposes all source cairo files in a mapping from file path to contents.
fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set."));

    #[cfg(feature = "dump_source_files")]
    dump_source::dump_source_files(&out_dir.join("cairo_files_map.json"));

    println!("cargo::warning=Compiling Starknet OS program...");
    let starknet_os_bytes = compile_program::compile_starknet_os();
    println!("cargo::warning=Done. Writing compiled bytes to output directory.");
    let starknet_os_bytes_path = out_dir.join("starknet_os_bytes");
    std::fs::write(&starknet_os_bytes_path, &starknet_os_bytes)
        .expect("Failed to write the compiled OS bytes to the output directory.");

    println!("cargo::warning=Compiling Starknet aggregator program...");
    let starknet_aggregator_bytes = compile_program::compile_starknet_aggregator();
    println!("cargo::warning=Done. Writing compiled bytes to output directory.");
    let starknet_aggregator_bytes_path = out_dir.join("starknet_aggregator_bytes");
    std::fs::write(&starknet_aggregator_bytes_path, &starknet_aggregator_bytes)
        .expect("Failed to write the compiled aggregator bytes to the output directory.");
}
