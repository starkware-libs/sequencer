use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_into_ap,
};
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::Felt252;
use num_bigint::BigUint;
use num_integer::Integer;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

/// Unpacks felt values into u32 arrays for Blake2s processing.
/// This implements the Cairo hint that converts felt values to u32 arrays
/// following the Blake2s encoding scheme.
pub(crate) fn unpack_felts_to_u32s(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let packed_values_len =
        get_integer_from_var_name("packed_values_len", vm, ids_data, ap_tracking)?;
    let packed_values = get_ptr_from_var_name("packed_values", vm, ids_data, ap_tracking)?;
    let unpacked_u32s = get_ptr_from_var_name("unpacked_u32s", vm, ids_data, ap_tracking)?;

    let vals = vm.get_integer_range(packed_values, felt_to_usize(&packed_values_len)?)?;
    let pow2_32 = BigUint::from(1_u32) << 32;
    let pow2_63 = BigUint::from(1_u32) << 63;
    let pow2_255 = BigUint::from(1_u32) << 255;

    // Split value into either 2 or 8 32-bit limbs.
    let out: Vec<MaybeRelocatable> = vals
        .into_iter()
        .map(|val| val.to_biguint())
        .flat_map(|val| {
            if val < pow2_63 {
                let (high, low) = val.div_rem(&pow2_32);
                vec![high, low]
            } else {
                let mut limbs = vec![BigUint::from(0_u32); 8];
                let mut val: BigUint = val + &pow2_255;
                for limb in limbs.iter_mut().rev() {
                    let (q, r) = val.div_rem(&pow2_32);
                    *limb = r;
                    val = q;
                }
                limbs
            }
        })
        .map(Felt252::from)
        .map(MaybeRelocatable::from)
        .collect();

    vm.load_data(unpacked_u32s, &out).map_err(HintError::Memory)?;
    Ok(())
}

/// Checks if we've reached the end of packed_values and if the current value is small (< 2^63).
/// This implements the Cairo hint that determines loop continuation and value size.
pub(crate) fn check_packed_values_end_and_size(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let end = get_ptr_from_var_name("end", vm, ids_data, ap_tracking)?;
    let packed_values = get_ptr_from_var_name("packed_values", vm, ids_data, ap_tracking)?;

    if end == packed_values {
        insert_value_into_ap(vm, 0)?
    } else {
        let val = vm.get_integer(packed_values)?;
        insert_value_into_ap(vm, (val.to_biguint() < (BigUint::from(1_u32) << 63)) as usize)?
    }
    Ok(())
}
