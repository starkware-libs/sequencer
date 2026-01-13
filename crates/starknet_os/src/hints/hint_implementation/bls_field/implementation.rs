use num_bigint::BigUint;
use starknet_types_core::felt::Felt;

use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Const, Ids};

/// From the Cairo code, we can make the current assumptions:
///
/// * The limbs of value are in the range [0, BASE * 3).
/// * value is in the range [0, 2 ** 256).
pub(crate) fn compute_ids_low<'program, CHP: CommonHintProcessor<'program>>(
    _hint_processor: &mut CHP,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let d0 = ctx.get_nested_field_felt(Ids::Value, CairoStruct::BigInt3, &["d0"])?;
    let d1 = ctx.get_nested_field_felt(Ids::Value, CairoStruct::BigInt3, &["d1"])?;
    let base = ctx.fetch_const(Const::Base)?;
    let mask = BigUint::from(u128::MAX);

    let low = (d0 + d1 * base).to_biguint() & mask;

    ctx.insert_value(Ids::Low, Felt::from(low))?;
    Ok(())
}
