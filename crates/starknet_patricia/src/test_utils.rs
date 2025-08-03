use std::collections::HashMap;

use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{try_extract_suffix_from_db_key, DbKeyPrefix};
use starknet_types_core::felt::Felt;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::filled_tree::node_serde::PatriciaPrefix;
use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;

// TODO(Nimrod): Remove once the committer has `fetch_witnesses` mechanism.
/// Filters inner nodes from the commitment storage for the commitment info that will be passed to
/// the OS.
/// Note: This produces many redundant commitments as the entire commitment storage will be
/// contained in each commitment info.
/// Result should be independent of the concrete leaf type, as all returned nodes are inner nodes.
pub fn filter_inner_nodes_from_commitments<L: Leaf>(
    commitments: &MapStorage,
) -> HashMap<HashOutput, Vec<Felt>> {
    let mut inner_nodes: HashMap<HashOutput, Vec<Felt>> = HashMap::new();
    let inner_node_prefix = DbKeyPrefix::from(PatriciaPrefix::InnerNode);
    for (key, value) in commitments.iter() {
        if let Some(suffix) = try_extract_suffix_from_db_key(key, &inner_node_prefix) {
            let is_leaf = false;
            let hash = HashOutput(Felt::from_bytes_be_slice(suffix));
            let node: FilledNode<L> = FilledNode::deserialize(hash, value, is_leaf).unwrap();
            let flatten_value = match node.data {
                NodeData::Binary(data) => data.flatten(),
                NodeData::Edge(data) => data.flatten(),
                NodeData::Leaf(_) => panic!("Expected an inner node, but found a leaf."),
            };
            inner_nodes.insert(hash, flatten_value);
        }
    }
    inner_nodes
}
