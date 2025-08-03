use std::collections::HashSet;
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_starknet_classes::NestedIntList;
use rstest::rstest;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::ContractClass;

use crate::execution::contract_class::{
    create_bytecode_segment_length_and_big_felt_count,
    CompiledClassV1,
    ContractClassV1Inner,
    NestedDoubleIntList,
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
        bytecode_segment_lengths: NestedIntList::Node(vec![
            NestedIntList::Leaf(151),
            NestedIntList::Leaf(104),
            NestedIntList::Node(vec![NestedIntList::Leaf(170), NestedIntList::Leaf(225)]),
            NestedIntList::Leaf(157),
            NestedIntList::Node(vec![NestedIntList::Node(vec![
                NestedIntList::Node(vec![NestedIntList::Leaf(101)]),
                NestedIntList::Leaf(195),
                NestedIntList::Leaf(125),
            ])]),
            NestedIntList::Leaf(162),
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
#[case::one_segment(
    NestedIntList::Leaf(3),
    vec![true, false, true],
    3,
    NestedDoubleIntList::Leaf(3, 2),
)]
#[case::simple_nested_structure(
    NestedIntList::Node(vec![
        NestedIntList::Leaf(3),
        NestedIntList::Leaf(2),
    ]),
    // Segment 1: [true, false, true] --> 2 big felts
    // Segment 2: [false, false] --> 0 big felts
    vec![true, false, true, false, false],
    // total length of the bytecode = 3 + 2 = 5
    5,
    NestedDoubleIntList::Node(vec![
        NestedDoubleIntList::Leaf(3, 2),
        NestedDoubleIntList::Leaf(2, 0),
    ])
)]
#[case::nested_structure(
    NestedIntList::Node(vec![
        NestedIntList::Node(vec![
            NestedIntList::Leaf(1),
            NestedIntList::Leaf(2),
        ]),
        NestedIntList::Leaf(2),
    ]),
    // Segment 1: [false] --> 0 big felts
    // Segment 2: [true, false] --> 1 big felt
    // Segment 3: [true, false] --> 1 big felt
    vec![false, true, false, true, false],
    // total length of the bytecode = 1 + 2 + 2 = 5
    5,
    NestedDoubleIntList::Node(vec![
        NestedDoubleIntList::Node(vec![
            NestedDoubleIntList::Leaf(1, 0),
            NestedDoubleIntList::Leaf(2, 1),
        ]),
        NestedDoubleIntList::Leaf(2, 1),
    ])
)]
fn test_create_combined_segment_structure_success(
    #[case] lengths: NestedIntList,
    #[case] big_felt_flags: Vec<bool>,
    #[case] expected_total_length: usize,
    #[case] expected_result: NestedDoubleIntList,
) {
    let result = create_bytecode_segment_length_and_big_felt_count(
        &lengths,
        &big_felt_flags,
        expected_total_length,
    );
    assert_eq!(result, expected_result);
}
