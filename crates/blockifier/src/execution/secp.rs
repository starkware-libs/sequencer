use ark_ec::short_weierstrass::{Affine, SWCurveConfig};
use ark_ff::{BigInteger, PrimeField, Zero};
use starknet_types_core::felt::Felt;

use crate::execution::syscalls::hint_processor::INVALID_ARGUMENT_FELT;
use crate::execution::syscalls::vm_syscall_utils::SyscallExecutorBaseError;

/// The OS secp helpers treat any point with x == 0 as the point at infinity. secp256r1 has a valid
/// affine point with x == 0 (secp256k1 does not), which blockifier must reject to stay consistent
/// with the OS.
pub trait SecpZeroXPolicy {
    const REJECT_ZERO_X_POINT: bool;
}

impl SecpZeroXPolicy for ark_secp256r1::Config {
    const REJECT_ZERO_X_POINT: bool = true;
}

impl SecpZeroXPolicy for ark_secp256k1::Config {
    const REJECT_ZERO_X_POINT: bool = false;
}

/// Rejects a point the OS would treat as the point at infinity (see [`SecpZeroXPolicy`]). Shared by
/// the VM and Cairo Native syscall paths so they stay consistent.
pub fn reject_zero_x_point<Curve: SWCurveConfig + SecpZeroXPolicy>(
    point: &Affine<Curve>,
) -> Result<(), SyscallExecutorBaseError> {
    if Curve::REJECT_ZERO_X_POINT && !point.infinity && point.x.is_zero() {
        return Err(SyscallExecutorBaseError::InvalidSyscallInput {
            input: Felt::ZERO,
            info: "secp256r1 points with x-coordinate 0 are not allowed".to_string(),
        });
    }
    Ok(())
}

pub fn get_point_from_x<Curve: SWCurveConfig>(
    x: num_bigint::BigUint,
    y_parity: bool,
) -> Result<Option<Affine<Curve>>, SyscallExecutorBaseError>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    modulus_bound_check::<Curve>(&[&x])?;

    let x = x.into();
    let maybe_ec_point = Affine::<Curve>::get_ys_from_x_unchecked(x)
        .map(|(smaller, greater)| {
            // Return the correct y coordinate based on the parity.
            if smaller.into_bigint().is_odd() == y_parity { smaller } else { greater }
        })
        .map(|y| Affine::<Curve>::new_unchecked(x, y))
        .filter(|p| p.is_in_correct_subgroup_assuming_on_curve());

    Ok(maybe_ec_point)
}

pub fn new_affine<Curve: SWCurveConfig>(
    x: num_bigint::BigUint,
    y: num_bigint::BigUint,
) -> Result<Option<Affine<Curve>>, SyscallExecutorBaseError>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    modulus_bound_check::<Curve>(&[&x, &y])?;

    Ok(maybe_affine(x.into(), y.into()))
}

fn modulus_bound_check<Curve: SWCurveConfig>(
    bounds: &[&num_bigint::BigUint],
) -> Result<(), SyscallExecutorBaseError>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    let modulus = Curve::BaseField::MODULUS.into();

    if bounds.iter().any(|p| **p >= modulus) {
        return Err(SyscallExecutorBaseError::Revert { error_data: vec![INVALID_ARGUMENT_FELT] });
    }

    Ok(())
}

/// Variation on [`Affine<Curve>::new`] that doesn't panic and maps (x,y) = (0,0) -> infinity
fn maybe_affine<Curve: SWCurveConfig>(
    x: Curve::BaseField,
    y: Curve::BaseField,
) -> Option<Affine<Curve>> {
    let ec_point = if x.is_zero() && y.is_zero() {
        Affine::<Curve>::identity()
    } else {
        Affine::<Curve>::new_unchecked(x, y)
    };

    if ec_point.is_on_curve() && ec_point.is_in_correct_subgroup_assuming_on_curve() {
        Some(ec_point)
    } else {
        None
    }
}
