use std::iter::repeat_with;
use std::sync::LazyLock;

use c_kzg::KzgCommitment;
use num_bigint::BigUint;
use num_traits::{Num, One, Zero};
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::kzg::utils::{
    bit_reversal,
    fft,
    polynomial_coefficients_to_blob,
    serialize_blob,
    split_commitment,
    BLOB_SUBGROUP_GENERATOR,
    BLS_PRIME,
    FIELD_ELEMENTS_PER_BLOB,
    WIDTH,
};

const BYTES_PER_BLOB: usize = FIELD_ELEMENTS_PER_BLOB * 32;
const GENERATOR: &str =
    "39033254847818212395286706435128746857159659164139250548781411570340225835782";

static FFT_REGRESSION_INPUT: LazyLock<Vec<BigUint>> = LazyLock::new(|| {
    serde_json::from_str::<Vec<String>>(include_str!("fft_regression_input.json"))
        .unwrap()
        .into_iter()
        .map(|s| BigUint::from_str_radix(&s, 10).unwrap())
        .collect()
});
static FFT_REGRESSION_OUTPUT: LazyLock<Vec<u8>> =
    LazyLock::new(|| serde_json::from_str(include_str!("fft_regression_output.json")).unwrap());
static BLOB_REGRESSION_INPUT: LazyLock<Vec<BigUint>> = LazyLock::new(|| {
    serde_json::from_str::<Vec<String>>(include_str!("blob_regression_input.json"))
        .unwrap()
        .into_iter()
        .map(|s| BigUint::from_str_radix(&s, 10).unwrap())
        .collect()
});
static BLOB_REGRESSION_OUTPUT: LazyLock<Vec<u8>> =
    LazyLock::new(|| serde_json::from_str(include_str!("blob_regression_output.json")).unwrap());

fn generate(generator: &BigUint, bit_reversed: bool) -> Vec<BigUint> {
    let mut array = vec![BigUint::one()];
    for _ in 1..FIELD_ELEMENTS_PER_BLOB {
        let last = array.last().unwrap().clone();
        let next = (generator * &last) % BigUint::from_str_radix(BLS_PRIME, 10).unwrap();
        array.push(next);
    }

    if bit_reversed {
        bit_reversal(&mut array, WIDTH as u32);
    }

    array
}

#[rstest]
fn test_small_fft_regression(#[values(true, false)] bit_reversed: bool) {
    let prime = BigUint::from(17_u8);
    let generator = BigUint::from(3_u8);
    let coeffs: Vec<BigUint> = [0_u8, 1, 2, 3].into_iter().map(BigUint::from).collect();
    let expected_eval: Vec<BigUint> =
        (if bit_reversed { [6_u8, 15, 9, 4] } else { [6_u8, 9, 15, 4] })
            .into_iter()
            .map(BigUint::from)
            .collect();
    let actual_eval = fft(&coeffs, &generator, &prime, bit_reversed).unwrap();
    assert_eq!(actual_eval, expected_eval);
}

#[rstest]
fn test_fft(#[values(true, false)] bit_reversed: bool) {
    let prime = BigUint::from_str_radix(BLS_PRIME, 10).unwrap();
    let generator = BigUint::from_str_radix(GENERATOR, 10).unwrap();

    let subgroup = generate(&generator, bit_reversed);

    // Sanity checks.
    assert_eq!(
        (&generator.modpow(&BigUint::from(FIELD_ELEMENTS_PER_BLOB), &prime)),
        &BigUint::one()
    );
    assert_eq!(subgroup.len(), subgroup.iter().collect::<std::collections::HashSet<_>>().len());

    let coeffs: Vec<BigUint> = repeat_with(|| BigUint::from(rand::random::<u64>()) % &prime)
        .take(FIELD_ELEMENTS_PER_BLOB)
        .collect();

    // Evaluate naively.
    let mut expected_eval = vec![BigUint::zero(); FIELD_ELEMENTS_PER_BLOB];
    for (i, x) in subgroup.iter().enumerate() {
        let eval = generate(x, false);
        expected_eval[i] =
            coeffs.iter().zip(eval.iter()).map(|(c, e)| c * e).sum::<BigUint>() % &prime;
    }

    // Evaluate using FFT.
    let actual_eval = fft(&coeffs, &generator, &prime, bit_reversed).unwrap();

    assert_eq!(actual_eval, expected_eval);

    // Trivial cases.
    assert_eq!(actual_eval[0], coeffs.iter().sum::<BigUint>() % &prime);
    assert_eq!(
        fft(&vec![BigUint::zero(); FIELD_ELEMENTS_PER_BLOB], &generator, &prime, bit_reversed)
            .unwrap(),
        vec![BigUint::zero(); FIELD_ELEMENTS_PER_BLOB]
    );
    assert_eq!(
        fft(&[BigUint::from(121212u64)], &BigUint::one(), &prime, bit_reversed).unwrap(),
        vec![BigUint::from(121212u64)]
    );
}

/// All the expected values are checked using the contract logic given in the Starknet core
/// contract:
/// https://github.com/starkware-libs/cairo-lang/blob/a86e92bfde9c171c0856d7b46580c66e004922f3/src/starkware/starknet/solidity/Starknet.sol#L209.
#[rstest]
#[case(
    BigUint::from_str_radix(
        "b7a71dc9d8e15ea474a69c0531e720cf5474b189ac9afc81590b91a225b1bf7fa5877ec546d090e0059f019c74675362",
        16,
    ).unwrap(),
    (
        Felt::from_hex_unchecked("590b91a225b1bf7fa5877ec546d090e0059f019c74675362"),
        Felt::from_hex_unchecked("b7a71dc9d8e15ea474a69c0531e720cf5474b189ac9afc81"),
    )
)]
#[case(
    BigUint::from_str_radix(
        "a797c1973c99961e357246ee81bde0acbdd27e801d186ccb051732ecbaa75842afd3d8860d40b3e8eeea433bce18b5c8",
        16,
    ).unwrap(),
    (
        Felt::from_hex_unchecked("51732ecbaa75842afd3d8860d40b3e8eeea433bce18b5c8"),
        Felt::from_hex_unchecked("a797c1973c99961e357246ee81bde0acbdd27e801d186ccb"),
    )
)]
fn test_split_commitment_function(
    #[case] commitment: BigUint,
    #[case] expected_output: (Felt, Felt),
) {
    let commitment = KzgCommitment::from_bytes(&commitment.to_bytes_be()).unwrap();
    assert_eq!(split_commitment(&commitment).unwrap(), expected_output);
}

#[rstest]
#[case::zero(vec![BigUint::zero()], &vec![0u8; BYTES_PER_BLOB])]
#[case::one(
    vec![BigUint::one()],
    &(0..BYTES_PER_BLOB).map(|i| if (i + 1) % 32 == 0 { 1 } else { 0 }).collect()
)]
#[case::degree_one(
    vec![BigUint::zero(), BigUint::from(10_u8)],
    &serialize_blob(
        &generate(&BigUint::from_str_radix(BLOB_SUBGROUP_GENERATOR, 10).unwrap(), true)
            .into_iter()
            .map(|subgroup_elm| (BigUint::from(10_u8) * subgroup_elm)
                % BigUint::from_str_radix(BLS_PRIME, 10).unwrap()
            )
            .collect::<Vec<BigUint>>(),
    ).unwrap()
)]
#[case::original(BLOB_REGRESSION_INPUT.to_vec(), &BLOB_REGRESSION_OUTPUT)]
#[case::generated(FFT_REGRESSION_INPUT.to_vec(), &FFT_REGRESSION_OUTPUT)]
fn test_fft_blob_regression(#[case] input: Vec<BigUint>, #[case] expected_output: &Vec<u8>) {
    let bytes = polynomial_coefficients_to_blob(input).unwrap();
    assert_eq!(&bytes, expected_output);
}
