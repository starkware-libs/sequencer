use num_bigint::BigUint;
use starknet_types_core::felt::Felt;

use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Const, Ids};
use crate::vm_utils::get_address_of_nested_fields;

/// From the Cairo code, we can make the current assumptions:
///
/// * The limbs of value are in the range [0, BASE * 3).
/// * value is in the range [0, 2 ** 256).
pub(crate) fn compute_ids_low<'program, CHP: CommonHintProcessor<'program>>(
    hint_processor: &mut CHP,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let d0 = ctx
        .vm
        .get_integer(get_address_of_nested_fields(
            ctx.ids_data,
            Ids::Value,
            CairoStruct::BigInt3,
            ctx.vm,
            ctx.ap_tracking,
            &["d0"],
            hint_processor.get_program(),
        )?)?
        .into_owned();
    let d1 = ctx
        .vm
        .get_integer(get_address_of_nested_fields(
            ctx.ids_data,
            Ids::Value,
            CairoStruct::BigInt3,
            ctx.vm,
            ctx.ap_tracking,
            &["d1"],
            hint_processor.get_program(),
        )?)?
        .into_owned();
    let base = Const::Base.fetch(ctx.constants)?;
    let mask = BigUint::from(u128::MAX);

    let low = (d0 + d1 * base).to_biguint() & mask;

    ctx.insert_value(Ids::Low, Felt::from(low))?;
    Ok(())
}
