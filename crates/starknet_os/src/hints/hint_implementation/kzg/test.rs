use std::iter::repeat_with;

use c_kzg::KzgCommitment;
use num_bigint::BigInt;
use num_traits::{Num, One, Zero};
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::kzg::utils::{fft, split_commitment, BLS_PRIME};

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
fn test_small_fft_regression(#[values(true, false)] bit_reversed: bool) {
    let prime = BigInt::from(17);
    let generator = BigInt::from(3);
    let coeffs: Vec<BigInt> = [0, 1, 2, 3].into_iter().map(BigInt::from).collect();
    let expected_eval: Vec<BigInt> = (if bit_reversed { [6, 15, 9, 4] } else { [6, 9, 15, 4] })
        .into_iter()
        .map(BigInt::from)
        .collect();
    let actual_eval = fft(&coeffs, &generator, &prime, bit_reversed).unwrap();
    assert_eq!(actual_eval, expected_eval);
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

/// All the expected values are checked using the contract logic given in the Starknet core
/// contract:
/// https://github.com/starkware-libs/cairo-lang/blob/a86e92bfde9c171c0856d7b46580c66e004922f3/src/starkware/starknet/solidity/Starknet.sol#L209.
#[rstest]
#[case(
    BigInt::from_str_radix(
        "b7a71dc9d8e15ea474a69c0531e720cf5474b189ac9afc81590b91a225b1bf7fa5877ec546d090e0059f019c74675362",
        16,
    ).unwrap(),
    (
        Felt::from_hex_unchecked("590b91a225b1bf7fa5877ec546d090e0059f019c74675362"),
        Felt::from_hex_unchecked("b7a71dc9d8e15ea474a69c0531e720cf5474b189ac9afc81"),
    )
)]
#[case(
    BigInt::from_str_radix(
        "a797c1973c99961e357246ee81bde0acbdd27e801d186ccb051732ecbaa75842afd3d8860d40b3e8eeea433bce18b5c8",
        16,
    ).unwrap(),
    (
        Felt::from_hex_unchecked("51732ecbaa75842afd3d8860d40b3e8eeea433bce18b5c8"),
        Felt::from_hex_unchecked("a797c1973c99961e357246ee81bde0acbdd27e801d186ccb"),
    )
)]
fn test_split_commitment_function(
    #[case] commitment: BigInt,
    #[case] expected_output: (Felt, Felt),
) {
    let commitment = KzgCommitment::from_bytes(&commitment.to_bytes_be().1).unwrap();
    assert_eq!(split_commitment(&commitment).unwrap(), expected_output);
}
