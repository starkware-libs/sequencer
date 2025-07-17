use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;
use num_bigint::BigUint;
use starknet_types_core::felt::Felt;

use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids};
use crate::vm_utils::get_address_of_nested_fields;

/// From the Cairo code, we can make the current assumptions:
///
/// * The limbs of value are in the range [0, BASE * 3).
/// * value is in the range [0, 2 ** 256).
pub(crate) fn compute_ids_low<'program, CHP: CommonHintProcessor<'program>>(
    hint_processor: &mut CHP,
    HintArgs { vm, ap_tracking, ids_data, constants, .. }: HintArgs<'_>,
) -> OsHintResult {
    let d0 = vm
        .get_integer(get_address_of_nested_fields(
            ids_data,
            Ids::Value,
            CairoStruct::BigInt3,
            vm,
            ap_tracking,
            &["d0"],
            hint_processor.get_program(),
        )?)?
        .into_owned();
    let d1 = vm
        .get_integer(get_address_of_nested_fields(
            ids_data,
            Ids::Value,
            CairoStruct::BigInt3,
            vm,
            ap_tracking,
            &["d1"],
            hint_processor.get_program(),
        )?)?
        .into_owned();
    let base = Const::Base.fetch(constants)?;
    let mask = BigUint::from(u128::MAX);

    let low = (d0 + d1 * base).to_biguint() & mask;

    insert_value_from_var_name(Ids::Low.into(), Felt::from(low), vm, ids_data, ap_tracking)?;
    Ok(())
}
