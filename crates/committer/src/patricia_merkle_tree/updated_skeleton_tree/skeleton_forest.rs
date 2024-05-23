use std::collections::HashMap;

use crate::block_committer::input::ContractAddress;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTree;

#[allow(dead_code)]
pub(crate) struct UpdatedSkeletonForest<T: UpdatedSkeletonTree> {
    #[allow(dead_code)]
    classes_trie: T,
    #[allow(dead_code)]
    contracts_trie: T,
    #[allow(dead_code)]
    storage_tries: HashMap<ContractAddress, T>,
}
