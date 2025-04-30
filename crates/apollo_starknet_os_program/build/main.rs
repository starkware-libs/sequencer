#[cfg(feature = "dump_source_files")]
use std::path::PathBuf;

#[cfg(feature = "dump_source_files")]
mod dump_source;

/// Build script for the `apollo_starknet_os_program` crate.
/// Recompiles the OS program if the source files change.
/// Optionally, also exposes all source cairo files in a mapping from file path to contents.
fn main() {
    #[cfg(feature = "dump_source_files")]
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set.");
    #[cfg(feature = "dump_source_files")]
    dump_source::dump_source_files(PathBuf::from(out_dir).join("cairo_files_map.json"));
}
