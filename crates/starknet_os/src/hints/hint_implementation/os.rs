use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use starknet_api::core::{ClassHash, Nonce};
use starknet_types_core::felt::Felt;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::Scope;

pub(crate) fn initialize_class_hashes<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let state_input = &hint_processor.execution_helper.cached_state;
    let class_hash_to_compiled_class_hash =
        state_input.cache.clone().into_inner().initial_reads.compiled_class_hashes;
    exec_scopes.insert_value(Scope::InitialDict.into(), class_hash_to_compiled_class_hash);
    Ok(())
}

// pub const INITIALIZE_STATE_CHANGES: &str = indoc! {r#"
//     from starkware.python.utils import from_bytes

//     initial_dict = {
//         address: segments.gen_arg(
//             (from_bytes(contract.contract_hash), segments.add(), contract.nonce))
//         for address, contract in os_input.contracts.items()
//     }"#
// };

// pub fn initialize_state_changes(
//     vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     _ids_data: &HashMap<String, HintReference>,
//     _ap_tracking: &ApTracking,
//     _constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError> { let os_input =
//   exec_scopes.get::<Rc<StarknetOsInput>>(vars::scopes::OS_INPUT)?; let mut state_dict:
//   HashMap<MaybeRelocatable, MaybeRelocatable> = HashMap::new(); for (addr, contract_state) in
//   &os_input.contracts { let change_base = vm.add_memory_segment(); vm.insert_value(change_base,
//   Felt252::from_bytes_be_slice(&contract_state.contract_hash))?; let storage_commitment_base =
//   vm.add_memory_segment(); vm.insert_value((change_base + 1)?, storage_commitment_base)?;
//   vm.insert_value((change_base + 2)?, contract_state.nonce)?;

//         state_dict.insert(MaybeRelocatable::from(addr), MaybeRelocatable::from(change_base));
//     }

//     exec_scopes.insert_box(vars::scopes::INITIAL_DICT, Box::new(state_dict));
//     Ok(())
// }

pub(crate) fn initialize_state_changes<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let contract_items = &hint_processor.execution_helper.cached_state.get_initial_reads()?;
    let hashes_and_nonces: Vec<(ClassHash, Nonce)> = contract_items
        .nonces
        .iter()
        .map(|(address, nonce)| {
            let contract_hash = contract_items.class_hashes.get(address).unwrap();
            (*contract_hash, *nonce)
        })
        .collect();
    // TODO(Aner): verify that it is not necessary to recursively iterate over the vector, and
    // insert the values to the vm one by one, or to call segments.add().
    exec_scopes.insert_value(Scope::InitialDict.into(), hashes_and_nonces);
    Ok(())
}

pub(crate) fn write_full_output_to_memory<S: StateReader>(
    HintArgs { vm, hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    let full_output = Felt::from(os_input.full_output);
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::WriteFullOutputToMemory), full_output)
}

pub(crate) fn configure_kzg_manager<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    // TODO(Aner): verify that inserting into the "root" scope is not neccessary.
    exec_scopes.insert_value(Scope::SerializeDataAvailabilityCreatePages.into(), true);
    Ok(())
}

pub(crate) fn set_ap_to_prev_block_hash<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    Ok(insert_value_into_ap(vm, os_input.prev_block_hash.0)?)
}

pub(crate) fn set_ap_to_new_block_hash<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    Ok(insert_value_into_ap(vm, os_input.new_block_hash.0)?)
}

pub(crate) fn starknet_os_input<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // Nothing to do here; OS input already available on the hint processor.
    Ok(())
}

pub(crate) fn init_state_update_pointer<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}
