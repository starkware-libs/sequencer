use std::collections::HashMap;

use committer::felt::Felt;
use committer::impl_from_hex_for_felt_wrapper;
use committer::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use committer::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use committer::patricia_merkle_tree::types::NodeIndex;
use starknet_types_core::felt::FromStrError;

use crate::block_committer::input::{ContractAddress, StarknetStorageValue};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;

// TODO(Nimrod, 1/6/2024): Use the ClassHash defined in starknet-types-core when available.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClassHash(pub Felt);

impl From<&ClassHash> for NodeIndex {
    fn from(val: &ClassHash) -> Self {
        NodeIndex::from_leaf_felt(&val.0)
    }
}

impl_from_hex_for_felt_wrapper!(ClassHash);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Nonce(pub Felt);

impl_from_hex_for_felt_wrapper!(Nonce);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompiledClassHash(pub Felt);

impl_from_hex_for_felt_wrapper!(CompiledClassHash);

pub type StorageTrie = FilledTreeImpl<StarknetStorageValue>;
pub type ClassesTrie = FilledTreeImpl<CompiledClassHash>;
pub type ContractsTrie = FilledTreeImpl<ContractState>;
pub type StorageTrieMap = HashMap<ContractAddress, StorageTrie>;

pub type StorageTrieError = FilledTreeError<StarknetStorageValue>;
pub type ClassesTrieError = FilledTreeError<CompiledClassHash>;
pub type ContractsTrieError = FilledTreeError<ContractState>;
