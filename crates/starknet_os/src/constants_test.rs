use cairo_vm::types::program::Program;
use starknet_types_core::felt::Felt;

#[allow(dead_code)]
fn get_from_program(program: &Program, const_path: &str) -> Felt {
    program
        .constants
        .get(const_path)
        .cloned()
        .unwrap_or_else(|| panic!("Constant {const_path} not found in the program."))
}
