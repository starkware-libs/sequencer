use starknet_api::core::ascii_as_felt;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::storage_trait::DbKey;
use starknet_types_core::felt::Felt;

// TODO (Ariel): Add block number when we manage historical tries.
#[derive(Debug, PartialEq, Default)]
pub struct KeyContext {
    pub trie_type: TrieType,
}

#[derive(Debug, PartialEq, Default)]
pub enum TrieType {
    ContractsTrie,
    ClassesTrie,
    StorageTrie(Felt),
    #[default]
    GeneralTrie,
}

impl TrieType {
    pub fn get_prefix(&self) -> Vec<u8> {
        match self {
            TrieType::ContractsTrie => {
                ascii_as_felt("CONTRACTS_TREE_PREFIX").unwrap().to_bytes_be().to_vec()
            }
            TrieType::ClassesTrie => {
                ascii_as_felt("CLASSES_TREE_PREFIX").unwrap().to_bytes_be().to_vec()
            }
            TrieType::StorageTrie(contract_address) => (*contract_address).to_bytes_be().to_vec(),
            TrieType::GeneralTrie => vec![],
        }
    }
}

pub fn db_key_from_node_index_and_context(node_index: NodeIndex, context: &KeyContext) -> DbKey {
    let prefix = context.trie_type.get_prefix();
    let key = prefix.into_iter().chain(node_index.0.to_be_bytes()).collect::<Vec<u8>>();
    DbKey(key)
}
