use starknet_types_core::felt::{Felt, NonZeroFelt};

#[cfg(test)]
#[path = "ec_test.rs"]
mod ec_test;

// Elliptic curve types and helpers.

/// Marker representing the point at infinity on the elliptic curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Infinity;

/// An elliptic curve point in affine coordinates (x, y).
pub type EcPoint = (Felt, Felt);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EcPointOrInfinity {
    Point(EcPoint),
    Infinity,
}

#[derive(Clone, Debug)]
pub struct NotOnCurveException;

/// Computes the slope of the line connecting the two given EC points over the Stark field.
/// Assumes the points are given in affine form (x, y) and have different x coordinates.
pub fn line_slope(point1: EcPoint, point2: EcPoint) -> Felt {
    let (x1, y1) = point1;
    let (x2, y2) = point2;
    let dx = NonZeroFelt::try_from(x1 - x2).expect("non-zero denominator");
    let dy = y1 - y2;
    // Compute (y1 - y2) / (x1 - x2) using field division.
    dy.as_ref().field_div(&dx)
}

/// Gets two points on an elliptic curve over the Stark field and returns their sum.
/// Assumes the points are given in affine form (x, y) and have different x coordinates.
pub fn ec_add(point1: EcPoint, point2: EcPoint) -> EcPoint {
    let m = line_slope(point1, point2);
    let x = m * m - point1.0 - point2.0;
    let y = m * (point1.0 - x) - point1.1;
    (x, y)
}

/// Computes the slope at the given point for a curve with equation y^2 = x^3 + alpha*x + beta.
/// Assumes the point is given in affine form (x, y) and has y != 0.
pub fn ec_double_slope(point: EcPoint, alpha: Felt) -> Felt {
    let (x, y) = point;
    assert!(y != Felt::ZERO);
    let numerator = Felt::from(3u8) * x * x + alpha;
    let denominator = NonZeroFelt::try_from(Felt::from(2u8) * y).expect("non-zero denominator");
    numerator.as_ref().field_div(&denominator)
}

/// Doubles a point on an elliptic curve with the equation y^2 = x^3 + alpha*x + beta.
/// Assumes the point is given in affine form (x, y) and has y != 0.
pub fn ec_double(point: EcPoint, alpha: Felt) -> EcPoint {
    let m = ec_double_slope(point, alpha);
    let x = m * m - Felt::from(2u8) * point.0;
    let y = m * (point.0 - x) - point.1;
    (x, y)
}

/// Gets two points on an elliptic curve over the Stark field and returns their sum.
/// Safe to use always. May get or return the point at infinity (represented as `Infinity`).
pub fn ec_safe_add(
    point1: EcPointOrInfinity,
    point2: EcPointOrInfinity,
    alpha: Felt,
) -> EcPointOrInfinity {
    match (point1, point2) {
        (EcPointOrInfinity::Infinity, other) => other,
        (other, EcPointOrInfinity::Infinity) => other,
        (EcPointOrInfinity::Point((x1, y1)), EcPointOrInfinity::Point((x2, y2))) => {
            if x1 == x2 {
                if y1 == Felt::ZERO - y2 {
                    EcPointOrInfinity::Infinity
                } else {
                    EcPointOrInfinity::Point(ec_double((x1, y1), alpha))
                }
            } else {
                EcPointOrInfinity::Point(ec_add((x1, y1), (x2, y2)))
            }
        }
    }
}

/// Multiplies by `m` a point on the elliptic curve with equation y^2 = x^3 + alpha*x + beta.
/// Assumes the point is given in affine form (x, y) and that 0 < m < order(point).
pub fn ec_mult(m: u128, point: EcPoint, alpha: Felt) -> EcPoint {
    if m == 1 {
        return point;
    }
    if m % 2 == 0 {
        return ec_mult(m / 2, ec_double(point, alpha), alpha);
    }
    ec_add(ec_mult(m - 1, point, alpha), point)
}

/// Multiplies by `m` a point on the elliptic curve with equation y^2 = x^3 + alpha*x + beta.
/// Assumes the point is given in affine form (x, y).
/// Safe to use always. May get or return the point at infinity (represented as `Infinity`).
pub fn ec_safe_mult(m: u128, point: EcPointOrInfinity, alpha: Felt) -> EcPointOrInfinity {
    if m == 0 {
        return EcPointOrInfinity::Infinity;
    }
    if m == 1 {
        return point;
    }
    if m % 2 == 0 {
        let doubled = ec_safe_add(point, point, alpha);
        return ec_safe_mult(m / 2, doubled, alpha);
    }
    let prev = ec_safe_mult(m - 1, point, alpha);
    ec_safe_add(prev, point, alpha)
}

/// Computes y^2 using the curve equation: y^2 = x^3 + alpha * x + beta.
pub fn y_squared_from_x(x: Felt, alpha: Felt, beta: Felt) -> Felt {
    x.pow(3_u128) + alpha * x + beta
}

/// Recovers the corresponding y coordinate on the elliptic curve.
/// y^2 = x^3 + alpha * x + beta of a given x coordinate.
/// Returns the minimal non-negative root when both exist.
pub fn recover_y(x: Felt, alpha: Felt, beta: Felt) -> Result<Felt, NotOnCurveException> {
    let y_squared_felt = y_squared_from_x(x, alpha, beta);
    y_squared_felt.sqrt().ok_or(NotOnCurveException)
}

fn find_point(alpha: Felt, beta: Felt, start: u64, require_nonzero_y: bool) -> EcPoint {
    let mut x = Felt::from(start);
    loop {
        if let Ok(y) = recover_y(x, alpha, beta) {
            if !require_nonzero_y || y != Felt::ZERO {
                return (x, y);
            }
        }
        x = x + Felt::ONE;
    }
}

