use ethnum::U256;
use rstest::rstest;

use crate::patricia_merkle_tree::node_data::inner_node::{EdgePathLength, PathToBottom};

#[rstest]
#[case(PathToBottom::from("1011"), 1, PathToBottom::from("011"))]
#[case(PathToBottom::from("1011"), 2, PathToBottom::from("11"))]
#[case(PathToBottom::from("1011"), 3, PathToBottom::from("1"))]
#[case(PathToBottom::from("1011"), 4, PathToBottom::new(U256::ZERO.into(), EdgePathLength::new(0).unwrap()).unwrap())]
#[should_panic]
#[case(PathToBottom::from("1011"), 5, PathToBottom::from("0"))]
fn test_remove_first_edges(
    #[case] path_to_bottom: PathToBottom,
    #[case] n_edges: u8,
    #[case] expected: PathToBottom,
) {
    assert_eq!(
        path_to_bottom.remove_first_edges(EdgePathLength::new(n_edges).unwrap()).unwrap(),
        expected
    );
}
