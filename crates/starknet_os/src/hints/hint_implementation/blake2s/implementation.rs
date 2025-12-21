use std::sync::LazyLock;

use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
};
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::errors::hint_errors::HintError;
use num_bigint::BigUint;
use num_integer::Integer;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

static POW2_32: LazyLock<BigUint> = LazyLock::new(|| BigUint::from(1_u32) << 32);

pub(crate) fn naive_unpack_felt252_to_u32s(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let mut packed_value =
        get_integer_from_var_name(Ids::PackedValue.into(), vm, ids_data, ap_tracking)?.to_biguint();
    let unpacked_u32s = get_ptr_from_var_name(Ids::UnpackedU32s.into(), vm, ids_data, ap_tracking)?;
    let mut limbs = vec![BigUint::from(0_u32); 8];
    for limb in limbs.iter_mut() {
        let (q, r) = packed_value.div_rem(&POW2_32);
        *limb = r;
        packed_value = q;
    }
    let out: Vec<MaybeRelocatable> =
        limbs.into_iter().map(Felt::from).map(MaybeRelocatable::from).collect();

    vm.load_data(unpacked_u32s, &out).map_err(HintError::Memory)?;
    Ok(())
}
