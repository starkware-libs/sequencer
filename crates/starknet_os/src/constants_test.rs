use cairo_vm::types::program::Program;
use starknet_types_core::felt::Felt;

#[allow(dead_code)]
fn get_from_program(program: &Program, const_name: &str) -> Felt {
    program
        .constants
        .get(const_name)
        .cloned()
        .unwrap_or_else(|| panic!("Constant {const_name} not found in the program."))
}
