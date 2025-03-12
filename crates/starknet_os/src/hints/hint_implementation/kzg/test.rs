use std::iter::repeat_with;

use num_bigint::BigInt;
use num_traits::{Num, One, Zero};
use rstest::rstest;

use crate::hints::hint_implementation::kzg::utils::{fft, BLS_PRIME};

const GENERATOR: &str =
    "39033254847818212395286706435128746857159659164139250548781411570340225835782";
const WIDTH: usize = 12;
const ORDER: usize = 1 << WIDTH;

fn generate(generator: &BigInt) -> Vec<BigInt> {
    let mut array = vec![BigInt::one()];
    for _ in 1..ORDER {
        let last = array.last().unwrap().clone();
        let next = (generator * &last) % BigInt::from_str_radix(BLS_PRIME, 10).unwrap();
        array.push(next);
    }
    array
}

#[rstest]
fn test_fft(#[values(true, false)] bit_reversed: bool) {
    let prime = BigInt::from_str_radix(BLS_PRIME, 10).unwrap();
    let generator = BigInt::from_str_radix(GENERATOR, 10).unwrap();

    let mut subgroup = generate(&generator);
    if bit_reversed {
        let perm: Vec<usize> = (0..ORDER)
            .map(|i| {
                let binary = format!("{:0width$b}", i, width = WIDTH);
                usize::from_str_radix(&binary.chars().rev().collect::<String>(), 2).unwrap()
            })
            .collect();
        subgroup = perm.iter().map(|&i| subgroup[i].clone()).collect();
    }

    // Sanity checks.
    assert_eq!((&generator.modpow(&BigInt::from(ORDER), &prime)), &BigInt::one());
    assert_eq!(subgroup.len(), subgroup.iter().collect::<std::collections::HashSet<_>>().len());

    let coeffs: Vec<BigInt> =
        repeat_with(|| BigInt::from(rand::random::<u64>()) % &prime).take(ORDER).collect();

    // Evaluate naively.
    let mut expected_eval = vec![BigInt::zero(); ORDER];
    for (i, x) in subgroup.iter().enumerate() {
        let eval = generate(x);
        expected_eval[i] =
            coeffs.iter().zip(eval.iter()).map(|(c, e)| c * e).sum::<BigInt>() % &prime;
    }

    // Evaluate using FFT.
    let actual_eval = fft(&coeffs, &generator, &prime, bit_reversed).unwrap();

    assert_eq!(actual_eval, expected_eval);

    // Trivial cases.
    assert_eq!(actual_eval[0], coeffs.iter().sum::<BigInt>() % &prime);
    assert_eq!(
        fft(&vec![BigInt::zero(); ORDER], &generator, &prime, bit_reversed).unwrap(),
        vec![BigInt::zero(); ORDER]
    );
    assert_eq!(
        fft(&[BigInt::from(121212u64)], &BigInt::one(), &prime, bit_reversed).unwrap(),
        vec![BigInt::from(121212u64)]
    );
}
