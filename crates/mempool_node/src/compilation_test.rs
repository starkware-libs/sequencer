use crate::compilation::compile_node_result;

#[test]
fn test_compile_node() {
    assert!(compile_node_result().is_ok(), "Compilation failed");
}
