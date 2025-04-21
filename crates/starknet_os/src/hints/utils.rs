use std::collections::HashMap;

use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::Felt252;

pub fn get_constant_from_complete_var_name<'a>(
    var_name: &'static str,
    constants: &'a HashMap<String, Felt252>,
) -> Result<&'a Felt252, HintError> {
    constants
        .iter()
        .find(|(k, _)| *k == var_name)
        .map(|(_, n)| n)
        .ok_or_else(|| HintError::MissingConstant(Box::new(var_name)))
}
