use std::collections::HashSet;
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_starknet_classes::NestedIntList;
use cairo_lang_utils::bigint::BigUintAsHex;
use rstest::rstest;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::ContractClass;
use starknet_types_core::felt::Felt;

use crate::execution::contract_class::{
    CompiledClassV1,
    ContractClassV1Inner,
    FeltSizeCount,
    NestedFeltCounts,
    RunnableCompiledClass,
};
use crate::test_utils::contracts::FeatureContractTrait;
use crate::transaction::errors::TransactionExecutionError;

#[rstest]
fn test_get_visited_segments() {
    let test_contract = CompiledClassV1(Arc::new(ContractClassV1Inner {
        program: Default::default(),
        entry_points_by_type: Default::default(),
        hints: Default::default(),
        sierra_version: Default::default(),
        bytecode_segment_felt_sizes: NestedFeltCounts::Node(vec![
            NestedFeltCounts::Leaf(151, FeltSizeCount { small: 151, large: 0 }),
            NestedFeltCounts::Leaf(104, FeltSizeCount { small: 104, large: 0 }),
            NestedFeltCounts::Node(vec![
                NestedFeltCounts::Leaf(170, FeltSizeCount { small: 170, large: 0 }),
                NestedFeltCounts::Leaf(225, FeltSizeCount { small: 225, large: 0 }),
            ]),
            NestedFeltCounts::Leaf(157, FeltSizeCount { small: 157, large: 0 }),
            NestedFeltCounts::Node(vec![NestedFeltCounts::Node(vec![
                NestedFeltCounts::Node(vec![NestedFeltCounts::Leaf(
                    101,
                    FeltSizeCount { small: 101, large: 0 },
                )]),
                NestedFeltCounts::Leaf(195, FeltSizeCount { small: 195, large: 0 }),
                NestedFeltCounts::Leaf(125, FeltSizeCount { small: 125, large: 0 }),
            ])]),
            NestedFeltCounts::Leaf(162, FeltSizeCount { small: 162, large: 0 }),
        ]),
    }));

    assert_eq!(
        test_contract
            .get_visited_segments(&HashSet::from([807, 907, 0, 1, 255, 425, 431, 1103]))
            .unwrap(),
        [0, 255, 425, 807, 1103]
    );

    assert_matches!(
        test_contract
            .get_visited_segments(&HashSet::from([907, 0, 1, 255, 425, 431, 1103]))
            .unwrap_err(),
        TransactionExecutionError::InvalidSegmentStructure(907, 807)
    );
}

/// Tests that the hash of the compiled contract class (CASM) matches the hash of the corresponding
/// runnable contract class.
#[rstest]
#[case(RunnableCairo1::Casm)]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native))]
fn test_compiled_class_hash(
    #[case] runnable_cairo_version: RunnableCairo1,
    #[values(HashVersion::V1, HashVersion::V2)] hash_version: HashVersion,
) {
    // Compute the hash of a Casm contract.
    let feature_contract =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let casm = match feature_contract.get_class() {
        ContractClass::V1((casm, _sierra_version)) => casm,

        _ => panic!("Expected ContractClass::V1"),
    };
    let casm_hash = casm.hash(&hash_version);

    // Compute the hash of a runnable contract.
    let runnable_feature_contract =
        FeatureContract::TestContract(CairoVersion::Cairo1(runnable_cairo_version));
    let runnable_contact_class = runnable_feature_contract.get_runnable_class();
    let runnable_contact_class_hash = match runnable_contact_class {
        RunnableCompiledClass::V1(runnable_contact_class) => {
            runnable_contact_class.hash(&hash_version)
        }
        #[cfg(feature = "cairo_native")]
        RunnableCompiledClass::V1Native(runnable_contact_class) => {
            runnable_contact_class.hash(&hash_version)
        }
        _ => panic!("RunnableCompiledClass::V0 does not support hash"),
    };
    assert_eq!(casm_hash, runnable_contact_class_hash);
}

#[rstest]
#[case::empty(
    NestedIntList::Leaf(0),
    vec![],
    NestedFeltCounts::Leaf(0, FeltSizeCount::default())
)]
#[case::leaf(
    NestedIntList::Leaf(3),
    vec![
        BigUintAsHex::from(1u64),
        BigUintAsHex::from(1u64 << 63),
        BigUintAsHex::from(1u64 << 63),
    ],
    NestedFeltCounts::Leaf(3, FeltSizeCount { small: 1, large: 2 })
)]
#[case::node(
    NestedIntList::Node(vec![
        NestedIntList::Leaf(1),
        NestedIntList::Leaf(2),
    ]),
    vec![
        BigUintAsHex::from(1u64),
        BigUintAsHex::from(1u64),
        BigUintAsHex::from(1u64 << 63),
    ],
    NestedFeltCounts::Node(vec![
        NestedFeltCounts::Leaf(1, FeltSizeCount { small: 1, large: 0 }),
        NestedFeltCounts::Leaf(2, FeltSizeCount { small: 1, large: 1 }),
    ])
)]
fn test_create_bytecode_segment_felt_sizes(
    #[case] bytecode_segment_lengths: NestedIntList,
    #[case] bytecode: Vec<BigUintAsHex>,
    #[case] expected_structure: NestedFeltCounts,
) {
    let result = NestedFeltCounts::new(&bytecode_segment_lengths, &bytecode);
    assert_eq!(result, expected_structure);
}

/// Creates a vector of `Felt` items, respecting the `FeltSizeCount` threshold.
fn generate_felt_items(n_small: usize, n_large: usize) -> Vec<Felt> {
    let small_felt = Felt::from((1u64 << 63) - 1);
    let large_felt = Felt::from(1u64 << 63);

    // Sanity check: threshold is respected.
    assert!(small_felt < FeltSizeCount::SMALL_THRESHOLD);
    assert!(large_felt >= FeltSizeCount::SMALL_THRESHOLD);

    let mut items = vec![small_felt; n_small];
    items.extend(vec![large_felt; n_large]);

    items
}

#[rstest]
#[case::empty(0, 0)]
#[case::small_and_large(2, 3)]
fn felt_size_count_from_felt_slice(#[case] expected_small: usize, #[case] expected_large: usize) {
    let items = generate_felt_items(expected_small, expected_large);

    // Convert the Felt slice to a FeltSizeCount:
    let count = FeltSizeCount::from(&items[..]);

    assert_eq!(count.small, expected_small);
    assert_eq!(count.large, expected_large);
}

#[rstest]
#[case::empty(0, 0)]
#[case::small_and_large(2, 3)]
fn felt_size_count_from_biguintashex_slice(
    #[case] expected_small: usize,
    #[case] expected_large: usize,
) {
    let felt_items = generate_felt_items(expected_small, expected_large);
    let biguint_items =
        felt_items.iter().map(|x| BigUintAsHex::from(x.to_biguint())).collect::<Vec<_>>();

    // Convert the BigUintAsHex slice to a FeltSizeCount:
    let count = FeltSizeCount::from(&biguint_items[..]);

    assert_eq!(count.small, expected_small);
    assert_eq!(count.large, expected_large);
}

#[rstest]
#[case::boundary_small_felt(&[Felt::from((1u64 << 63) - 1)])]
#[case::boundary_at_2_63(&[Felt::from(1u64 << 63)])]
#[case::very_large_felt(&[Felt::from_hex("0x800000000000011000000000000000000000000000000000000000000000000").unwrap()])]
#[case::mixed_small_large(&[Felt::from(42), Felt::from(1u64 << 63), Felt::from(1337)])]
#[case::many_large(&[Felt::from(1u64 << 63); 100])]
fn test_encoded_u32_len(#[case] test_data: &[Felt]) {
    use blake2s::encode_felts_to_u32s;

    let estimated_u32_len = FeltSizeCount::from(test_data).encoded_u32_len();
    let actual_u32_len = encode_felts_to_u32s(test_data.to_vec()).len();

    assert_eq!(actual_u32_len, estimated_u32_len);
}
