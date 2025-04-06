use num_traits::ToPrimitive;

/// Multiplies `multiplicand` by a fractional `factor` using fixed-point arithmetic.
///
/// Assumes `factor` is a non-negative decimal less than 1.0, with at most three decimal digits of
/// precision. In other words, `factor` must be between 0.0 (inclusive) and 1.0 (exclusive), and
/// should be representable as N / 1000, where N is an integer.
///
/// Uses a scale factor of 1,000 to emulate fixed-point math.
/// Rounds the result down to the nearest integer.
pub fn functional_mul(multiplicand: u128, factor: f64) -> u128 {
    const SCALE: u128 = 1_000;
    let scaled_multiplier = (factor * SCALE.to_f64().expect("Failed to convert SCALE to f64."))
        .to_u128()
        .expect("Failed to convert gas price multiplier");
    multiplicand.saturating_mul(scaled_multiplier) / SCALE
}
