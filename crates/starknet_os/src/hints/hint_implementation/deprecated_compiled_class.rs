use std::any::Any;
use std::collections::{HashMap, HashSet};

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;
use starknet_api::core::ClassHash;

use crate::hints::error::{OsHintExtensionResult, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};

pub(crate) fn load_deprecated_class_facts<S: StateReader>(
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    // TODO(Rotem): see if we can avoid cloning here.
    let deprecated_class_hashes: HashSet<ClassHash> =
        HashSet::from_iter(os_input.deprecated_compiled_classes.keys().cloned());
    exec_scopes.insert_value(Scope::DeprecatedClassHashes.into(), deprecated_class_hashes);

    insert_value_from_var_name(
        Ids::NCompiledClassFacts.into(),
        os_input.deprecated_compiled_classes.len(),
        vm,
        ids_data,
        ap_tracking,
    )?;
    let scoped_classes: Box<dyn Any> =
        Box::new(os_input.deprecated_compiled_classes.clone().into_iter());
    exec_scopes
        .enter_scope(HashMap::from([(Scope::CompiledClassFacts.to_string(), scoped_classes)]));

    Ok(())
}

pub(crate) fn load_deprecated_class_inner<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn load_deprecated_class<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintExtensionResult {
    todo!()
}
