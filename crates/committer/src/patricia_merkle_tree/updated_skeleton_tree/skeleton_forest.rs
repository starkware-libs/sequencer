use std::collections::HashMap;
use std::marker::PhantomData;

use crate::block_committer::input::ContractAddress;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTree;

#[allow(dead_code)]
pub(crate) struct UpdatedSkeletonForest<L: LeafData + std::clone::Clone, T: UpdatedSkeletonTree<L>>
{
    #[allow(dead_code)]
    classes_tree: T,
    #[allow(dead_code)]
    global_state_tree: T,
    #[allow(dead_code)]
    contract_states: HashMap<ContractAddress, T>,
    leaf_data: PhantomData<L>,
}
