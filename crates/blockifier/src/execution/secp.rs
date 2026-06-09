use ark_ec::short_weierstrass::{Affine, SWCurveConfig};
use ark_ff::{BigInteger, PrimeField, Zero};

use crate::execution::syscalls::hint_processor::INVALID_ARGUMENT_FELT;
use crate::execution::syscalls::vm_syscall_utils::SyscallExecutorBaseError;

/// The OS secp helpers special-case any point whose x-coordinate is zero as the point at infinity
/// (see `ec_double` / `fast_ec_add` in the cairo secp libraries). secp256r1 has a valid affine
/// point with x == 0 and y != 0, which the OS would therefore handle inconsistently, so blockifier
/// rejects it. secp256k1 has no affine point with x == 0 (`y^2 = 7` has no root modulo its prime)
/// and is unaffected.
pub trait SecpZeroXPolicy {
    /// Whether a non-infinity affine point with x == 0 must be rejected, because the OS would
    /// otherwise treat it as the point at infinity.
    const REJECT_ZERO_X_POINT: bool;
}

impl SecpZeroXPolicy for ark_secp256r1::Config {
    const REJECT_ZERO_X_POINT: bool = true;
}

impl SecpZeroXPolicy for ark_secp256k1::Config {
    const REJECT_ZERO_X_POINT: bool = false;
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
