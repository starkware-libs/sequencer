use crate::felt::Felt;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;

#[cfg(test)]
#[path = "types_test.rs"]
pub mod types_test;

#[allow(dead_code)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    derive_more::Add,
    derive_more::Mul,
    derive_more::Sub,
    PartialOrd,
    Ord,
)]
pub(crate) struct NodeIndex(pub Felt);

#[allow(dead_code)]
impl NodeIndex {
    pub(crate) fn root_index() -> NodeIndex {
        NodeIndex(Felt::ONE)
    }

    // TODO(Amos, 1/5/2024): Move to EdgePath.
    pub(crate) fn compute_bottom_index(
        index: NodeIndex,
        path_to_bottom: &PathToBottom,
    ) -> NodeIndex {
        let PathToBottom { path, length } = path_to_bottom;
        index.times_two_to_the_power(length.0) + NodeIndex(path.0)
    }

    pub(crate) fn times_two_to_the_power(&self, power: u8) -> Self {
        NodeIndex(self.0.times_two_to_the_power(power))
    }
}

impl From<u128> for NodeIndex {
    fn from(value: u128) -> Self {
        Self(Felt::from(value))
    }
}

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq, derive_more::Sub)]
pub(crate) struct TreeHeight(pub u8);
