use std::path::PathBuf;

mod compile_program;
mod dump_source;

/// Build script for the `apollo_starknet_os_program` crate.
/// Recompiles the OS program if the source files change.
/// Optionally, also exposes all source cairo files in a mapping from file path to contents.
#[tokio::main]
async fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set."));
<<<<<<< HEAD
||||||| 38f03e1d0

    #[cfg(feature = "dump_source_files")]
=======

>>>>>>> origin/main-v0.14.0
    dump_source::dump_source_files(&out_dir.join("cairo_files_map.json"));

    let mut task_set = tokio::task::JoinSet::new();
    #[cfg(feature = "test_programs")]
    task_set.spawn(compile_program::compile_test_contracts(out_dir.clone()));
    task_set.spawn(compile_program::compile_and_output_program(
        out_dir.clone(),
        "starkware/starknet/core/os/os.cairo",
        "starknet_os",
    ));
    task_set.spawn(compile_program::compile_and_output_program(
        out_dir,
        "starkware/starknet/core/aggregator/main.cairo",
        "starknet_aggregator",
    ));
    task_set.join_all().await;
}
