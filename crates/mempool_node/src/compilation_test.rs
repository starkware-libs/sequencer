use crate::compilation::compile_node_with_status;

#[test]
fn test_compile_node() {
    assert!(compile_node_with_status(), "Compilation failed");
}
