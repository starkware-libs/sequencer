use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

pub(crate) fn log2_ceil(mut ctx: HintArgs<'_>) -> OsHintResult {
    let value = ctx.get_integer(Ids::Value)?;
    assert!(value != Felt::ZERO, "log2_ceil is not defined for zero.");
    let bits = (value - Felt::ONE).bits();
    ctx.insert_value(Ids::Res, bits)?;
    Ok(())
}
