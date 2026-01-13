use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_lang_runner::short_string::as_cairo_short_string;
use cairo_vm::any_box;
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Ids, Scope};

pub(crate) fn selected_builtins(ctx: HintContext<'_>) -> OsHintResult {
    let n_selected_builtins = ctx.get_integer(Ids::NSelectedBuiltins)?;
    let new_scope =
        HashMap::from([(Scope::NSelectedBuiltins.into(), any_box!(n_selected_builtins))]);
    ctx.exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn select_builtin(mut ctx: HintContext<'_>) -> OsHintResult {
    let n_selected_builtins: Felt = ctx.get_from_scope(Scope::NSelectedBuiltins)?;
    let selected_encodings_ptr = ctx.get_ptr(Ids::SelectedEncodings)?;
    let all_encodings_ptr = ctx.get_ptr(Ids::AllEncodings)?;
    let select_builtin = n_selected_builtins != Felt::ZERO // Equivalent to n_selected_builtins > 0.
        && ctx.vm.get_integer(selected_encodings_ptr)? == ctx.vm.get_integer(all_encodings_ptr)?;
    if select_builtin {
        ctx.insert_into_scope(Scope::NSelectedBuiltins, n_selected_builtins - Felt::ONE);
    }
    ctx.insert_value(Ids::SelectBuiltin, Felt::from(select_builtin))?;

    Ok(())
}

/// Update subsets of the pointer at 'builtin_ptrs' with the pointers at 'selected_ptrs' according
/// to the location specified by 'selected_encodings'.
///
/// Assumption: selected builtins encoding is an ordered subset of builtin_params.
pub(crate) fn update_builtin_ptrs<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let n_builtins = ctx.get_integer(Ids::NBuiltins)?;

    let builtins_encoding = ctx.get_nested_field_ptr(
        Ids::BuiltinParams,
        CairoStruct::BuiltinParamsPtr,
        &["builtin_encodings"],
    )?;

    let n_selected_builtins = ctx.get_integer(Ids::NSelectedBuiltins)?;

    let selected_encodings = ctx.get_ptr(Ids::SelectedEncodings)?;

    let builtin_ptrs = ctx.get_ptr(Ids::BuiltinPtrs)?;

    let orig_builtin_ptrs = builtin_ptrs;

    let selected_ptrs = ctx.get_ptr(Ids::SelectedPtrs)?;

    let all_builtins =
        ctx.vm.get_continuous_range(builtins_encoding, felt_to_usize(&n_builtins)?)?;

    let selected_builtins =
        ctx.vm.get_continuous_range(selected_encodings, felt_to_usize(&n_selected_builtins)?)?;

    let mut returned_builtins: Vec<MaybeRelocatable> = Vec::new();
    let mut selected_builtin_offset: usize = 0;

    for (i, builtin) in all_builtins.iter().enumerate() {
        // For debugging purposes, try to decode the builtin name - ignore failures.
        let decoded_builtin: Option<String> = match builtin {
            MaybeRelocatable::Int(builtin_value) => as_cairo_short_string(builtin_value),
            MaybeRelocatable::RelocatableValue(builtin_ptr) => ctx
                .vm
                .get_integer(*builtin_ptr)
                .map(|felt| as_cairo_short_string(&felt))
                .unwrap_or(None),
        };
        if selected_builtins.contains(builtin) {
            // TODO(Dori): consider computing the address of the selected builtin via the
            //   `get_address_of_nested_fields` utility. See `OsLogger::insert_builtins` for an
            //   example for accessing fields of `CairoStruct::BuiltinPointers`.
            returned_builtins.push(
                ctx.vm.get_maybe(&(selected_ptrs + selected_builtin_offset)?).ok_or_else(|| {
                    OsHintError::MissingSelectedBuiltinPtr {
                        builtin: builtin.clone(),
                        decoded: decoded_builtin,
                    }
                })?,
            );
            selected_builtin_offset += 1;
        } else {
            // The builtin is unselected, hence its value is the same as before calling the program.
            returned_builtins.push(ctx.vm.get_maybe(&(orig_builtin_ptrs + i)?).ok_or_else(
                || OsHintError::MissingUnselectedBuiltinPtr {
                    builtin: builtin.clone(),
                    decoded: decoded_builtin,
                },
            )?);
        }
    }

    let return_builtin_ptrs_base = ctx.vm.add_memory_segment();
    ctx.vm.load_data(return_builtin_ptrs_base, &returned_builtins)?;
    ctx.insert_value(Ids::ReturnBuiltinPtrs, return_builtin_ptrs_base)?;
    Ok(())
}
