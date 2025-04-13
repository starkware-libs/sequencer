#[cfg(feature = "dump_source_files")]
use std::collections::HashMap;
use std::sync::LazyLock;

use cairo_vm::types::program::Program;

#[cfg(feature = "dump_source_files")]
pub static CAIRO_FILES_MAP: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/cairo_files_map.json")))
        .unwrap_or_else(|error| panic!("Failed to deserialize cairo_files_map.json: {error:?}."))
});

pub static OS_PROGRAM: LazyLock<Program> = LazyLock::new(|| {
    Program::from_bytes(
        include_bytes!(concat!(env!("OUT_DIR"), "/starknet_os_bytes")),
        Some("main"),
    )
    .expect("Failed to load the OS bytes.")
});
