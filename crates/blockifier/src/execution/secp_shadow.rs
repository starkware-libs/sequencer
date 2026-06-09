//! Extra input validation for the secp256r1 `add` / `mul` syscalls: rejects an operation that
//! involves a point with x-coordinate 0.
//!
//! The functions below mirror the secp256r1 EC routines in
//! `starkware/cairo/common/secp256r1/ec.cairo` so they visit the same sequence of points; they only
//! inspect those points and never affect the syscall's computed result. Each `fn` here mirrors the
//! cairo `func` of the same name — keep them in sync.

use ark_ec::short_weierstrass::{Affine, Projective, SWCurveConfig};
use ark_ff::{PrimeField, Zero};
use num_bigint::BigUint;

use crate::execution::secp::{zero_x_point_error, SecpZeroXPolicy};
use crate::execution::syscalls::vm_syscall_utils::SyscallExecutorBaseError;

type ShadowResult<T> = Result<T, SyscallExecutorBaseError>;

/// Whether `point` has affine x-coordinate 0 and is not the point at infinity. Tested on the
/// projective `X` coordinate to avoid an inversion: the affine `x` is `X` times a nonzero factor,
/// so it is zero iff `X == 0` and the point is not at infinity.
fn is_zero_x_point<Curve: SWCurveConfig>(point: &Projective<Curve>) -> bool {
    !point.is_zero() && point.x.is_zero()
}

/// Rejects a non-infinity point with x-coordinate 0.
fn reject_zero_x_operand<Curve: SWCurveConfig + SecpZeroXPolicy>(
    point: &Projective<Curve>,
) -> ShadowResult<()>
where
    Curve::BaseField: PrimeField,
{
    if is_zero_x_point(point) {
        return Err(zero_x_point_error());
    }
    Ok(())
}

/// Mirror of `ec_double`.
fn ec_double<Curve: SWCurveConfig + SecpZeroXPolicy>(
    point: Projective<Curve>,
) -> ShadowResult<Projective<Curve>>
where
    Curve::BaseField: PrimeField,
{
    reject_zero_x_operand(&point)?;
    Ok(point + point)
}

/// Mirror of both `ec_add` and `fast_ec_add`, which are identical here: only the two operands are
/// inspected. Every `ec_add` branch (`fast_ec_add`, `ec_double`, or the point at infinity) operates
/// on `point0` / `point1`, and the point at infinity is naturally distinguished in projective form
/// (`Z == 0` vs `X == 0`).
fn ec_add<Curve: SWCurveConfig + SecpZeroXPolicy>(
    point0: Projective<Curve>,
    point1: Projective<Curve>,
) -> ShadowResult<Projective<Curve>>
where
    Curve::BaseField: PrimeField,
{
    reject_zero_x_operand(&point0)?;
    reject_zero_x_operand(&point1)?;
    Ok(point0 + point1)
}

/// Mirror of `build_ec_mul_table`: returns `table[i] = i * point` for `i` in `0..=15`.
fn build_ec_mul_table<Curve: SWCurveConfig + SecpZeroXPolicy>(
    point: Projective<Curve>,
) -> ShadowResult<[Projective<Curve>; 16]>
where
    Curve::BaseField: PrimeField,
{
    // table[0] is the point at infinity.
    let mut table = [Projective::<Curve>::zero(); 16];
    table[1] = point;
    table[2] = ec_double(table[1])?;
    // The cairo unrolls table[3..=15]; table[i] = fast_ec_add(table[i - 1], point).
    for i in 3..16 {
        table[i] = ec_add(table[i - 1], point)?;
    }
    Ok(table)
}

/// Mirror of `fast_ec_mul_inner`: for each nibble (consumed most significant first), multiplies the
/// accumulator by 16 (four doublings) and adds the nibble's table entry.
fn fast_ec_mul_inner<Curve: SWCurveConfig + SecpZeroXPolicy>(
    table: &[Projective<Curve>; 16],
    mut point: Projective<Curve>,
    nibbles: impl Iterator<Item = usize>,
) -> ShadowResult<Projective<Curve>>
where
    Curve::BaseField: PrimeField,
{
    for nibble in nibbles {
        for _ in 0..4 {
            point = ec_double(point)?;
        }
        point = ec_add(point, table[nibble])?;
    }
    Ok(point)
}

/// The 64 nibbles (4 bits each) of the 256-bit scalar, `nibbles[0]` least significant.
fn scalar_nibbles(scalar: &BigUint) -> [usize; 64] {
    let bytes = scalar.to_bytes_le();
    core::array::from_fn(|k| {
        let byte = bytes.get(k / 2).copied().unwrap_or(0);
        usize::from(if k.is_multiple_of(2) { byte & 0x0f } else { byte >> 4 })
    })
}

/// Mirror of `ec_mul_by_uint256`: a 16-entry precompute table plus a windowed double-and-add over
/// the scalar's 64 nibbles, most significant first.
fn ec_mul_by_uint256<Curve: SWCurveConfig + SecpZeroXPolicy>(
    point: &Affine<Curve>,
    scalar: &BigUint,
) -> ShadowResult<Projective<Curve>>
where
    Curve::BaseField: PrimeField,
{
    let table = build_ec_mul_table(Projective::from(*point))?;
    let nibbles = scalar_nibbles(scalar);

    // first_nibble = nibbles[63], last_nibble = nibbles[0].
    let mut res = table[nibbles[63]];
    // First inner call (cairo m = 124): nibbles 62..=32, most significant first.
    res = fast_ec_mul_inner(&table, res, (32..=62).rev().map(|k| nibbles[k]))?;
    // Second inner call (cairo m = 124): nibbles 31..=1.
    res = fast_ec_mul_inner(&table, res, (1..=31).rev().map(|k| nibbles[k]))?;
    // Final window for the least-significant nibble: 16 * res, then ec_add with its table entry.
    for _ in 0..4 {
        res = ec_double(res)?;
    }
    ec_add(res, table[nibbles[0]])
}

/// Rejects a `secp256r1_add(point0, point1)` syscall that involves a point with x-coordinate 0.
/// No-op for curves without [`SecpZeroXPolicy::REJECT_ZERO_X_POINT`].
pub fn reject_zero_x_in_add<Curve: SWCurveConfig + SecpZeroXPolicy>(
    point0: &Affine<Curve>,
    point1: &Affine<Curve>,
) -> ShadowResult<()>
where
    Curve::BaseField: PrimeField,
{
    if !Curve::REJECT_ZERO_X_POINT {
        return Ok(());
    }
    // Only the rejection matters; the computed point is discarded.
    ec_add(Projective::from(*point0), Projective::from(*point1)).map(|_| ())
}

/// Rejects a `secp256r1_mul(point, scalar)` syscall that involves a point with x-coordinate 0 at
/// any point in the computation. No-op for curves without [`SecpZeroXPolicy::REJECT_ZERO_X_POINT`].
pub fn reject_zero_x_in_mul<Curve: SWCurveConfig + SecpZeroXPolicy>(
    point: &Affine<Curve>,
    scalar: &BigUint,
) -> ShadowResult<()>
where
    Curve::BaseField: PrimeField,
{
    if !Curve::REJECT_ZERO_X_POINT {
        return Ok(());
    }
    // Only the rejection matters; the computed point is discarded.
    ec_mul_by_uint256(point, scalar).map(|_| ())
}

#[cfg(test)]
mod tests {
    use ark_ec::short_weierstrass::Affine;
    use num_bigint::BigUint;

    use super::{reject_zero_x_in_add, reject_zero_x_in_mul};

    fn secp256r1_point(x_hex: &str, y_hex: &str) -> Affine<ark_secp256r1::Config> {
        let parse = |hex: &str| BigUint::parse_bytes(hex.as_bytes(), 16).unwrap();
        Affine::new_unchecked(parse(x_hex).into(), parse(y_hex).into())
    }

    fn zero_x_point() -> Affine<ark_secp256r1::Config> {
        secp256r1_point("0", "66485c780e2f83d72433bd5d84a06bb6541c2af31dae871728bf856a174f93f4")
    }

    fn generator() -> Affine<ark_secp256r1::Config> {
        secp256r1_point(
            "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296",
            "4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5",
        )
    }

    /// mul rejects a point with x-coordinate 0 that is reached early in the computation.
    #[test]
    fn mul_rejects_zero_x_point() {
        let half = secp256r1_point(
            "81bfb55b010b1bdf08b8d9d8590087aa278e28febff3b05632eeff09011c5579",
            "8cd2f199d9815d7585073034eb76c93d50799b354b0fb1e77eb75eba8bff3d58",
        );
        for scalar in [3_u32, 5, 7] {
            assert!(reject_zero_x_in_mul(&half, &BigUint::from(scalar)).is_err());
        }
        let quarter = secp256r1_point(
            "02495ac9f43e45aae30d3366e351cc08828cf3e11cc3b7209fbd1730c4a14f4e",
            "b55567231f26b356bbac703086b614bb21448433dd75ab263264c3d12206b9ee",
        );
        assert!(reject_zero_x_in_mul(&quarter, &BigUint::from(7_u32)).is_err());
    }

    /// mul rejects a point with x-coordinate 0 reached only deep in the computation.
    #[test]
    fn mul_rejects_zero_x_point_large_scalar() {
        let sixteenth = secp256r1_point(
            "776aef1acb82b628e132cc29440988f0a15d4cc2b4f328aecb063c9b86e5018e",
            "6e44dfc60444faa9c4e36bc217451f7ac2956cb3b2e9bbd655eba297163d1f34",
        );
        let scalar = BigUint::from(1_u8) << 252_u32;
        assert!(reject_zero_x_in_mul(&sixteenth, &scalar).is_err());
    }

    /// mul accepts a multiplication that never involves a point with x-coordinate 0.
    #[test]
    fn mul_accepts_regular_point() {
        assert!(reject_zero_x_in_mul(&generator(), &BigUint::from(5_u32)).is_ok());
    }

    /// add rejects an operand with x-coordinate 0 but not regular points.
    #[test]
    fn add_rejects_zero_x_operand() {
        assert!(reject_zero_x_in_add(&generator(), &generator()).is_ok());
        assert!(reject_zero_x_in_add(&zero_x_point(), &generator()).is_err());
        assert!(reject_zero_x_in_add(&generator(), &zero_x_point()).is_err());
    }
}
