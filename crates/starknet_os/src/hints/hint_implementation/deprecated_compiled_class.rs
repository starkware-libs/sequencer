use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;
use cairo_vm::Felt252;

use crate::hints::error::{HintExtensionResult, HintResult};
use crate::hints::types::HintArgs;
use crate::io::os_input::StarknetOsInput;

pub(crate) fn load_deprecated_class_facts<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> HintResult {
    let os_input = exec_scopes.get::<Rc<StarknetOsInput>>(vars::scopes::OS_INPUT)?;
    let deprecated_class_hashes: HashSet<Felt252> =
        HashSet::from_iter(os_input.deprecated_compiled_classes.keys().cloned());
    exec_scopes.insert_value(vars::scopes::DEPRECATED_CLASS_HASHES, deprecated_class_hashes);

    insert_value_from_var_name(vars::ids::COMPILED_CLASS_FACTS, vm.add_memory_segment(), vm, ids_data, ap_tracking)?;
    insert_value_from_var_name(
        vars::ids::N_COMPILED_CLASS_FACTS,
        os_input.deprecated_compiled_classes.len(),
        vm,
        ids_data,
        ap_tracking,
    )?;
    let scoped_classes: Box<dyn Any> = Box::new(os_input.deprecated_compiled_classes.clone().into_iter());
    exec_scopes.enter_scope(HashMap::from([(String::from(vars::scopes::COMPILED_CLASS_FACTS), scoped_classes)]));

    Ok(())
}

pub(crate) fn load_deprecated_class_inner<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn load_deprecated_class<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintExtensionResult {
    todo!()
}
