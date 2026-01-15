use ark_ff::{BigInteger, MontConfig};
use ark_secp256k1::FqConfig;
use blockifier::state::state_api::StateReader;
use num_bigint::BigInt;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Ids, Scope};

pub(crate) fn is_on_curve(mut ctx: HintContext<'_>) -> OsHintResult {
    let secp_p = BigInt::from_bytes_be(num_bigint::Sign::Plus, &FqConfig::MODULUS.to_bytes_be());
    let y: BigInt = ctx.get_from_scope(Scope::Y)?;
    let y_square_int: BigInt = ctx.get_from_scope(Scope::YSquareInt)?;

    let is_on_curve = ((y.pow(2)) % secp_p) == y_square_int;
    ctx.insert_value(Ids::IsOnCurve, Felt::from(is_on_curve))?;

    Ok(())
}

pub(crate) fn read_ec_point_from_address<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let not_on_curve = ctx.get_integer(Ids::NotOnCurve)?;
    let result = if not_on_curve == Felt::ZERO {
        ctx.get_nested_field_ptr(Ids::Response, CairoStruct::SecpNewResponsePtr, &["ec_point"])?
    } else {
        ctx.vm.add_memory_segment()
    };
    ctx.insert_value(Ids::ResultPtr, result)?;
    Ok(())
}
