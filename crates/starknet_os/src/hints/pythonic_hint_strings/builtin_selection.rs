use indoc::indoc;

pub(crate) const SELECTED_BUILTINS: &str =
    "vm_enter_scope({'n_selected_builtins': ids.n_selected_builtins})";

pub(crate) const SELECT_BUILTIN: &str = indoc! {r##"
# A builtin should be selected iff its encoding appears in the selected encodings list
# and the list wasn't exhausted.
# Note that testing inclusion by a single comparison is possible since the lists are sorted.
ids.select_builtin = int(
  n_selected_builtins > 0 and memory[ids.selected_encodings] == memory[ids.all_encodings])
if ids.select_builtin:
  n_selected_builtins = n_selected_builtins - 1"##
};
