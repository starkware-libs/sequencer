use starknet_api::core::ContractAddress;
use starknet_patricia::db_layout::NodeLayoutFor;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia_storage::db_object::{EmptyKeyContext, HasStaticPrefix};

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

/// A trait that specifies, for every leaf type, its db representation and the node layout for it.
pub trait DbLayout {
    type ContractStateDbLeaf: Leaf + HasStaticPrefix<KeyContext = EmptyKeyContext>;

    type CompiledClassHashDbLeaf: Leaf + HasStaticPrefix<KeyContext = EmptyKeyContext>;

    type StarknetStorageValueDbLeaf: Leaf + HasStaticPrefix<KeyContext = ContractAddress>;

    type NodeLayout: NodeLayoutFor<ContractState, DbLeaf = Self::ContractStateDbLeaf>
        + NodeLayoutFor<CompiledClassHash, DbLeaf = Self::CompiledClassHashDbLeaf>
        + NodeLayoutFor<StarknetStorageValue, DbLeaf = Self::StarknetStorageValueDbLeaf>;
}
