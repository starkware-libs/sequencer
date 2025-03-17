use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_constant_from_var_name,
    insert_value_from_var_name,
};
use num_bigint::BigUint;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids};
use crate::vm_utils::get_address_of_nested_fields;

// /// From the Cairo code, we can make the current assumptions:
// ///
// /// * The limbs of value are in the range [0, BASE * 3).
// /// * value is in the range [0, 2 ** 256).
// pub fn compute_ids_low(
//     vm: &mut VirtualMachine,
//     _exec_scopes: &mut ExecutionScopes,
//     ids_data: &HashMap<String, HintReference>,
//     ap_tracking: &ApTracking,
//     constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError> { let value_ptr = get_relocatable_from_var_name(vars::ids::VALUE, vm,
//   ids_data, ap_tracking)?; let d0 = vm.get_integer((value_ptr + BigInt3::d0_offset())?)?; let d1
//   = vm.get_integer((value_ptr + BigInt3::d1_offset())?)?;

//     let base = get_constant(vars::constants::BASE, constants)?;

//     let mask = (BigUint::from(1u64) << 128) - BigUint::from(1u64);
//     let low = (d0.as_ref() + d1.as_ref() * base).to_biguint() & mask;

//     insert_value_from_var_name(vars::ids::LOW, Felt252::from(low), vm, ids_data, ap_tracking)?;

//     Ok(())
// }

// pub const COMPUTE_IDS_LOW: &str = indoc! {r#"
//     ids.low = (ids.value.d0 + ids.value.d1 * ids.BASE) & ((1 << 128) - 1)"#
// };

/// From the Cairo code, we can make the current assumptions:
///
/// * The limbs of value are in the range [0, BASE * 3).
/// * value is in the range [0, 2 ** 256).
pub(crate) fn compute_ids_low<S: StateReader>(
    HintArgs { hint_processor, vm, ap_tracking, ids_data, constants, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let d0 = vm
        .get_integer(get_address_of_nested_fields(
            ids_data,
            Ids::Value,
            CairoStruct::StorageReadRequestPtr,
            vm,
            ap_tracking,
            &["d0".to_string()],
            &hint_processor.execution_helper.os_program,
        )?)?
        .into_owned();
    let d1 = vm
        .get_integer(get_address_of_nested_fields(
            ids_data,
            Ids::Value,
            CairoStruct::StorageReadRequestPtr,
            vm,
            ap_tracking,
            &["d1".to_string()],
            &hint_processor.execution_helper.os_program,
        )?)?
        .into_owned();
    let base = get_constant_from_var_name(Const::Base.into(), constants)?;
    let mask = (BigUint::from(1u64) << 128) - BigUint::from(1u64);

    let low = (d0 + d1 * base).to_biguint() & mask;

    insert_value_from_var_name(Ids::Low.into(), Felt::from(low), vm, ids_data, ap_tracking)?;
    Ok(())
}
