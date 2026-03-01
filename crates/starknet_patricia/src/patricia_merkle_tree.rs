pub mod errors;
pub mod filled_tree;
pub mod node_data;
pub mod original_skeleton_tree;
pub mod traversal;
pub mod types;
pub mod updated_skeleton_tree;

#[cfg(test)]
pub mod internal_test_utils;

#[cfg(any(feature = "testing", test))]
pub mod external_test_utils;

pub const DEFAULT_PATRICIA_NODE_PREFIX: &[u8] = b"patricia_node";
pub const DEFAULT_DB_KEY_SEPARATOR: &[u8] = b":";
