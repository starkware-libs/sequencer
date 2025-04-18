use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::compile_cairo0_program;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use cairo_vm::types::program::Program;

// TODO(Dori): Consider sharing this with the same function in this crate's build.rs.
fn cairo_root_path() -> PathBuf {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo")
}

pub fn compile_os_module(path_to_module: &PathBuf) -> Vec<u8> {
    compile_cairo0_program(path_to_module, &cairo_root_path())
        .unwrap_or_else(|error| panic!("Failed to compile module {path_to_module:?}: {error}."))
}

/// Compiles and deserializes a specific module from the OS, for unit testing.
pub fn compile_os_module_as_program(
    path_to_module: &PathBuf,
    main_entry_point: Option<&str>,
) -> Program {
    Program::from_bytes(&compile_os_module(path_to_module), main_entry_point).unwrap()
}
