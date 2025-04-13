use std::sync::LazyLock;

use cairo_vm::types::program::Program;

use crate::program_hash::ProgramHash;

pub mod program_hash;

pub static OS_PROGRAM: LazyLock<Program> = LazyLock::new(|| {
    Program::from_bytes(
        include_bytes!(concat!(env!("OUT_DIR"), "/starknet_os_bytes")),
        Some("main"),
    )
    .expect("Failed to load the OS bytes.")
});

pub static PROGRAM_HASH: LazyLock<ProgramHash> = LazyLock::new(|| {
    serde_json::from_str(include_str!("program_hash.json"))
        .expect("Failed to deserialize program_hash.json.")
});
