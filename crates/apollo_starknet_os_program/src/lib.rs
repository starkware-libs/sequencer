use std::sync::LazyLock;

use cairo_vm::types::program::Program;

pub static OS_PROGRAM: LazyLock<Program> = LazyLock::new(|| {
    Program::from_bytes(
        include_bytes!(concat!(env!("OUT_DIR"), "/starknet_os_bytes")),
        Some("main"),
    )
    .expect("Failed to load the OS bytes.")
});
