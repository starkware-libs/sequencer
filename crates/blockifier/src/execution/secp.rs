use ark_ec::short_weierstrass::{Affine, SWCurveConfig};
use ark_ff::{PrimeField, Zero};
use starknet_types_core::felt::Felt;

use super::syscalls::hint_processor::{SyscallExecutionError, INVALID_ARGUMENT};

pub fn get_point_from_x<Curve: SWCurveConfig>(
    x: num_bigint::BigUint,
    y_parity: bool,
) -> Result<Option<Affine<Curve>>, SyscallExecutionError>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    modulus_bound_check::<Curve>(&[&x])?;

    let x = x.into();
    let maybe_ec_point = Affine::<Curve>::get_ys_from_x_unchecked(x)
        .map(|(smaller, greater)| {
            // Return the correct y coordinate based on the parity.
            if ark_ff::BigInteger::is_odd(&smaller.into_bigint()) == y_parity {
                smaller
            } else {
                greater
            }
        })
        .map(|y| Affine::<Curve>::new_unchecked(x, y))
        .filter(|p| p.is_in_correct_subgroup_assuming_on_curve());

    Ok(maybe_ec_point)
}

pub fn new_affine<Curve: SWCurveConfig>(
    x: num_bigint::BigUint,
    y: num_bigint::BigUint,
) -> Result<Option<Affine<Curve>>, SyscallExecutionError>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    modulus_bound_check::<Curve>(&[&x, &y])?;

    Ok(maybe_affine(x.into(), y.into()))
}

fn modulus_bound_check<Curve: SWCurveConfig>(
    bounds: &[&num_bigint::BigUint],
) -> Result<(), SyscallExecutionError>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    let modulus = Curve::BaseField::MODULUS.into();

    if bounds.iter().any(|p| **p >= modulus) {
        let error = match Felt::from_hex(INVALID_ARGUMENT) {
            Ok(err) => SyscallExecutionError::SyscallError { error_data: vec![err] },
            Err(err) => SyscallExecutionError::from(err),
        };

        return Err(error);
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
