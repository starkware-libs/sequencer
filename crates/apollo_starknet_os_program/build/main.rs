use std::path::PathBuf;

mod compile_program;
mod dump_source;
mod virtual_os_utils;

/// Build script for the `apollo_starknet_os_program` crate.
/// Recompiles the OS program if the source files change.
/// Optionally, also exposes all source cairo files in a mapping from file path to contents.
#[tokio::main]
async fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set."));
    dump_source::dump_source_files(&out_dir.join("cairo_files_map.json"));

    // Prepare virtual cairo root (must be done before spawning tasks to avoid race conditions).
    let virtual_cairo_root = virtual_os_utils::prepare_virtual_cairo_root()
        .expect("Failed to prepare virtual cairo root");

    // Write the list of swapped files to OUT_DIR for use in tests.
    let swapped_files_json = serde_json::to_string(&virtual_cairo_root.swapped_files).unwrap();
    std::fs::write(out_dir.join("virtual_os_swapped_files.json"), swapped_files_json)
        .expect("Failed to write virtual_os_swapped_files.json");

    let mut task_set = tokio::task::JoinSet::new();
    #[cfg(feature = "test_programs")]
    task_set.spawn(compile_program::compile_test_contracts(out_dir.clone()));
    task_set.spawn(compile_program::compile_and_output_program(
        out_dir.clone(),
        "starkware/starknet/core/os/os.cairo",
        "starknet_os",
        None,
    ));
    task_set.spawn(compile_program::compile_and_output_program(
        out_dir.clone(),
        "starkware/starknet/core/aggregator/main.cairo",
        "starknet_aggregator",
        None,
    ));
    task_set.spawn(compile_program::compile_and_output_program(
        out_dir,
        "starkware/starknet/core/os/os.cairo",
        "virtual_os",
        Some(virtual_cairo_root.temp_dir.path().to_path_buf()),
    ));
    task_set.join_all().await;
}
