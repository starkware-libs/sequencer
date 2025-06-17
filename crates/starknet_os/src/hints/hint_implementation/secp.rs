use ark_ff::{BigInteger, MontConfig};
use ark_secp256k1::FqConfig;
use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use num_bigint::BigInt;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::get_address_of_nested_fields;

pub(crate) fn is_on_curve(
    HintArgs { exec_scopes, vm, ap_tracking, ids_data, .. }: HintArgs<'_>,
) -> OsHintResult {
    let secp_p = BigInt::from_bytes_be(num_bigint::Sign::Plus, &FqConfig::MODULUS.to_bytes_be());
    let y: BigInt = exec_scopes.get(Scope::Y.into())?;
    let y_square_int: BigInt = exec_scopes.get(Scope::YSquareInt.into())?;

    let is_on_curve = ((y.pow(2)) % secp_p) == y_square_int;
    insert_value_from_var_name(
        Ids::IsOnCurve.into(),
        Felt::from(is_on_curve),
        vm,
        ids_data,
        ap_tracking,
    )?;

    Ok(())
}

pub(crate) fn read_ec_point_from_address<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { ids_data, ap_tracking, vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let not_on_curve =
        get_integer_from_var_name(Ids::NotOnCurve.into(), vm, ids_data, ap_tracking)?;
    let ec_point_address = get_address_of_nested_fields(
        ids_data,
        Ids::Response,
        CairoStruct::SecpNewResponsePtr,
        vm,
        ap_tracking,
        &["ec_point"],
        hint_processor.program,
    )?;
    let result = if not_on_curve == Felt::ZERO {
        vm.get_relocatable(ec_point_address)?
    } else {
        vm.add_memory_segment()
    };
    insert_value_into_ap(vm, result)?;
    Ok(())
}
