use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub const SELECTED_BUILTINS: &str =
    "vm_enter_scope({'n_selected_builtins': ids.n_selected_builtins})";
pub fn selected_builtins(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub fn select_builtin(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub fn update_builtin_ptrs(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}
