use std::collections::HashSet;
use std::sync::Arc;

use assert_matches::assert_matches;
use cairo_lang_starknet_classes::NestedIntList;
use rstest::rstest;
use starknet_api::contract_class::{ContractClassV1, ContractClassV1Inner};

use crate::execution::contract_class::ContractClassV1Ext;
use crate::transaction::errors::TransactionExecutionError;

#[rstest]
fn test_get_visited_segments() {
    let test_contract = ContractClassV1(Arc::new(ContractClassV1Inner::new_for_testing(
        Default::default(),
        Default::default(),
        Default::default(),
        NestedIntList::Node(vec![
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
    )));

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
