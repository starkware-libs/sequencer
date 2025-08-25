use std::sync::LazyLock;

use ark_bls12_381::Fr;
use c_kzg::KzgCommitment;
use num_bigint::BigUint;
use num_traits::{Num, One, Zero};
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::kzg::utils::{
    bit_reversal,
    deserialize_blob,
    polynomial_coefficients_to_blob,
    serialize_blob,
    split_commitment,
    BLS_PRIME,
    FIELD_ELEMENTS_PER_BLOB,
};

static BLOB_SUBGROUP_GENERATOR: LazyLock<BigUint> = LazyLock::new(|| {
    BigUint::from_str_radix(
        "39033254847818212395286706435128746857159659164139250548781411570340225835782",
        10,
    )
    .unwrap()
});
const BYTES_PER_BLOB: usize = FIELD_ELEMENTS_PER_BLOB * 32;

static FFT_REGRESSION_INPUT: LazyLock<Vec<Fr>> = LazyLock::new(|| {
    serde_json::from_str::<Vec<String>>(include_str!("fft_regression_input.json"))
        .unwrap()
        .into_iter()
        .map(|s| Fr::from(BigUint::from_str_radix(&s, 10).unwrap()))
        .collect()
});
static FFT_REGRESSION_OUTPUT: LazyLock<Vec<u8>> =
    LazyLock::new(|| serde_json::from_str(include_str!("fft_regression_output.json")).unwrap());
static BLOB_REGRESSION_INPUT: LazyLock<Vec<Fr>> = LazyLock::new(|| {
    serde_json::from_str::<Vec<String>>(include_str!("blob_regression_input.json"))
        .unwrap()
        .into_iter()
        .map(|s| Fr::from(BigUint::from_str_radix(&s, 10).unwrap()))
        .collect()
});
static BLOB_REGRESSION_OUTPUT: LazyLock<Vec<u8>> =
    LazyLock::new(|| serde_json::from_str(include_str!("blob_regression_output.json")).unwrap());

fn generate(generator: &BigUint) -> Vec<BigUint> {
    let mut array = vec![BigUint::one()];
    for _ in 1..FIELD_ELEMENTS_PER_BLOB {
        let last = array.last().unwrap().clone();
        let next = (generator * &last) % &*BLS_PRIME;
        array.push(next);
    }
    bit_reversal(&mut array).unwrap();

    array
}

#[test]
fn test_blob_bytes_serde() {
    let serded_blob = deserialize_blob(&serialize_blob(&*BLOB_REGRESSION_INPUT).unwrap()).unwrap();
    assert_eq!(*BLOB_REGRESSION_INPUT, serded_blob);
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
#[case::zero(vec![Fr::zero()], &vec![0u8; BYTES_PER_BLOB])]
#[case::one(
    vec![Fr::one()], &(0..BYTES_PER_BLOB).map(|i| if (i + 1) % 32 == 0 { 1 } else { 0 }).collect()
)]
#[case::degree_one(
    vec![Fr::zero(), Fr::from(10_u8)],
    &serialize_blob(
        &generate(&BLOB_SUBGROUP_GENERATOR)
            .into_iter()
            .map(|subgroup_elm| Fr::from((BigUint::from(10_u8) * subgroup_elm) % &*BLS_PRIME))
            .collect::<Vec<Fr>>(),
    ).unwrap()
)]
#[case::original(BLOB_REGRESSION_INPUT.to_vec(), &BLOB_REGRESSION_OUTPUT)]
#[case::generated(FFT_REGRESSION_INPUT.to_vec(), &FFT_REGRESSION_OUTPUT)]
fn test_fft_blob_regression(#[case] input: Vec<Fr>, #[case] expected_output: &Vec<u8>) {
    let bytes = polynomial_coefficients_to_blob(input).unwrap();
    assert_eq!(&bytes, expected_output);
}
