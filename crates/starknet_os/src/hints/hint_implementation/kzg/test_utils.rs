use num_bigint::BigUint;

pub(crate) fn horner_eval(coefficients: &[BigUint], point: &BigUint, prime: &BigUint) -> BigUint {
    coefficients.iter().rev().fold(BigUint::ZERO, |acc, coeff| (acc * point + coeff) % prime)
}
