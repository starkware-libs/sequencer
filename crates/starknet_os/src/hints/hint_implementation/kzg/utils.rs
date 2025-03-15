use num_bigint::BigInt;

/// Performs the recursive Fast Fourier Transform (FFT) on the input coefficient vector `coeffs`
/// using the provided group elements `group` and modulus `prime`.
///
/// # Arguments
///
/// * `coeffs` - A slice of `BigInt` representing the coefficients of the polynomial.
/// * `group` - A slice of `BigInt` representing the precomputed group elements for the FFT.
/// * `prime` - A `BigInt` representing the prime modulus for the field operations.
///
/// # Returns
///
/// A `Vec<BigInt>` containing the transformed coefficients after applying the FFT.
///
/// # See More
/// - https://en.wikipedia.org/wiki/Fast_Fourier_transform
/// - https://github.com/starkware-libs/cairo-lang/blob/v0.13.2/src/starkware/python/math_utils.py#L310
fn inner_fft(coeffs: &[BigInt], group: &[BigInt], prime: &BigInt) -> Vec<BigInt> {
    if coeffs.len() == 1 {
        return coeffs.to_vec();
    }

    // These calls involve a clone and a collect
    // This not cheap and can possibly be improved by using dynamic iterators or transform the function to make it not recursive
    let f_even = inner_fft(
        &coeffs.iter().step_by(2).cloned().collect::<Vec<_>>(),
        &group.iter().step_by(2).cloned().collect::<Vec<_>>(),
        prime,
    );
    let f_odd = inner_fft(
        &coeffs.iter().skip(1).step_by(2).cloned().collect::<Vec<_>>(),
        &group.iter().step_by(2).cloned().collect::<Vec<_>>(),
        prime,
    );

    let group_mul_f_odd: Vec<BigInt> =
        group.iter().take(f_odd.len()).zip(f_odd.iter()).map(|(g, f)| (g * f) % prime).collect();

    let mut result = Vec::with_capacity(coeffs.len());
    for i in 0..f_even.len() {
        result.push((f_even[i].clone() + &group_mul_f_odd[i]) % prime);
    }
    for i in 0..f_even.len() {
        // Ensure non-negative diff by adding prime to the value before applying the modulo
        let diff = (f_even[i].clone() - &group_mul_f_odd[i] + prime) % prime;
        result.push(diff);
    }

    result
}
