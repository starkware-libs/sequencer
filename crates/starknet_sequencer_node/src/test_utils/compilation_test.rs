use rstest::rstest;

use crate::test_utils::compilation::compile_node_result;

#[rstest]
#[tokio::test]
async fn test_compile_node() {
    assert!(compile_node_result().await.is_ok(), "Compilation failed");
}
