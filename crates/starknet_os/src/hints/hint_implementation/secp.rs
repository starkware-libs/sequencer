use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn is_on_curve(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn read_ec_point_from_address(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}
