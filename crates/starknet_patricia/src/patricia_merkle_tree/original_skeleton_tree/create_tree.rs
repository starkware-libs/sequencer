use std::collections::HashMap;

use starknet_api::hash::HashOutput;

use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
