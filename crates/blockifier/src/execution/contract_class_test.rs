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
    CompiledClassV1,
    ContractClassV1Inner,
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
#[case(HashVersion::V1)]
#[case(HashVersion::V2)]
fn test_compiled_class_hash(#[case] hash_version: HashVersion) {
    let feature_contract =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let casm = match feature_contract.get_class() {
        ContractClass::V1((casm, _sierra_version)) => casm,
        _ => panic!("Expected ContractClass::V1"),
    };
    let casm_hash = casm.hash(Some(&hash_version));

    let runnable_contact_class = feature_contract.get_runnable_class();
    let runnable_contact_class_hash = match runnable_contact_class {
        RunnableCompiledClass::V1(runnable_contact_class) => {
            runnable_contact_class.hash(Some(&hash_version))
        }
        _ => panic!("Expected RunnableCompiledClass::V1"),
    };
    assert_eq!(casm_hash, runnable_contact_class_hash);
}
