use starknet_api::core::{ClassHash, Nonce};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::errors::LeafResult;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia_storage::db_object::{DBObject, HasStaticPrefix};
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::storage_trait::{DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

// Wrapper types for the leaves so that we can implement a different DBObject for them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IndexLayoutContractState(pub ContractState);
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IndexLayoutCompiledClassHash(pub CompiledClassHash);
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IndexLayoutStarknetStorageValue(pub StarknetStorageValue);

impl From<IndexLayoutContractState> for ContractState {
    fn from(index_layout_contract_state: IndexLayoutContractState) -> Self {
        index_layout_contract_state.0
    }
}

impl AsRef<ContractState> for IndexLayoutContractState {
    fn as_ref(&self) -> &ContractState {
        &self.0
    }
}

impl From<StarknetStorageValue> for IndexLayoutStarknetStorageValue {
    fn from(starknet_storage_value: StarknetStorageValue) -> Self {
        IndexLayoutStarknetStorageValue(starknet_storage_value)
    }
}

impl From<CompiledClassHash> for IndexLayoutCompiledClassHash {
    fn from(compiled_class_hash: CompiledClassHash) -> Self {
        IndexLayoutCompiledClassHash(compiled_class_hash)
    }
}

impl AsRef<CompiledClassHash> for IndexLayoutCompiledClassHash {
    fn as_ref(&self) -> &CompiledClassHash {
        &self.0
    }
}

#[derive(Debug, PartialEq)]
pub enum TrieType {
    ContractsTrie,
    ClassesTrie,
    StorageTrie(Felt),
}

impl From<&TrieType> for DbKeyPrefix {
    fn from(trie_type: &TrieType) -> Self {
        match trie_type {
            TrieType::ContractsTrie => DbKeyPrefix::new(b"CONTRACTS_TREE_PREFIX"),
            TrieType::ClassesTrie => DbKeyPrefix::new(b"CLASSES_TREE_PREFIX"),
            TrieType::StorageTrie(contract_address) => {
                DbKeyPrefix::new(&(contract_address).to_bytes_be())
            }
        }
    }
}

// TODO(Ariel): Remove this macro  when HasStaticPrefix is a local trait. Replace with a blanket
// impl for any type that implements some dummy IndexLayoutStaticPrefix trait.
macro_rules! impl_has_static_prefix_for_index_layouts {
    ($($ty:ty),* $(,)?) => {
        $(
            impl HasStaticPrefix for $ty {
                type KeyContext = TrieType;
                fn get_static_prefix(key_context: &Self::KeyContext) -> DbKeyPrefix {
                    key_context.into()
                }
            }
        )*
    };
}

impl_has_static_prefix_for_index_layouts! {
    IndexLayoutContractState,
    IndexLayoutCompiledClassHash,
    IndexLayoutStarknetStorageValue,
}

macro_rules! impl_leaf_for_wrappers {
    ($($wrapper:ty => $inner:ty),+ $(,)?) => {
        $(
            impl Leaf for $wrapper {
                type Input = <$inner as Leaf>::Input;
                type Output = <$inner as Leaf>::Output;

                fn is_empty(&self) -> bool {
                    // assumes `pub struct Wrapper(pub Inner);`
                    self.0.is_empty()
                }

                async fn create(
                    input: Self::Input,
                ) -> LeafResult<(Self, Self::Output)> {
                    let (inner, output) = <$inner as Leaf>::create(input).await?;
                    Ok((Self(inner), output))
                }
            }
        )+
    };
}

impl_leaf_for_wrappers!(
    IndexLayoutContractState => ContractState,
    IndexLayoutStarknetStorageValue => StarknetStorageValue,
    IndexLayoutCompiledClassHash => CompiledClassHash,
);

#[derive(Debug)]
pub struct IndexLayoutLeafDeserializationError(pub &'static str);

impl std::fmt::Display for IndexLayoutLeafDeserializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for IndexLayoutLeafDeserializationError {}

impl DBObject for IndexLayoutContractState {
    type DeserializeContext = ();
    fn serialize(&self) -> DbValue {
        let mut buffer = Vec::new();
        self.0.class_hash.0.serialize(&mut buffer).unwrap();
        self.0.storage_root_hash.0.serialize(&mut buffer).unwrap();
        self.0.nonce.0.serialize(&mut buffer).unwrap();
        DbValue(buffer)
    }

    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        let mut cursor: &[u8] = &value.0;
        let error_msg = "failed to deserialize ContractState from index DB";
        let class_hash = Felt::deserialize(&mut cursor).ok_or(DeserializationError::ValueError(
            Box::new(IndexLayoutLeafDeserializationError(error_msg)),
        ))?;
        let storage_root_hash =
            Felt::deserialize(&mut cursor).ok_or(DeserializationError::ValueError(Box::new(
                IndexLayoutLeafDeserializationError(error_msg),
            )))?;
        let nonce = Felt::deserialize(&mut cursor).ok_or(DeserializationError::ValueError(
            Box::new(IndexLayoutLeafDeserializationError(error_msg)),
        ))?;
        Ok(IndexLayoutContractState(ContractState {
            class_hash: ClassHash(class_hash),
            storage_root_hash: HashOutput(storage_root_hash),
            nonce: Nonce(nonce),
        }))
    }
}

impl DBObject for IndexLayoutCompiledClassHash {
    type DeserializeContext = ();
    fn serialize(&self) -> DbValue {
        let mut buffer = Vec::new();
        self.0.0.serialize(&mut buffer).unwrap();
        DbValue(buffer)
    }
    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        Ok(CompiledClassHash(Felt::deserialize(&mut &value.0[..]).ok_or(
            DeserializationError::ValueError(Box::new(IndexLayoutLeafDeserializationError(
                "failed to deserialize CompiledClassHash from index DB",
            ))),
        )?)
        .into())
    }
}

impl DBObject for IndexLayoutStarknetStorageValue {
    type DeserializeContext = ();
    fn serialize(&self) -> DbValue {
        let mut buffer = Vec::new();
        self.0.0.serialize(&mut buffer).unwrap();
        DbValue(buffer)
    }
    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        Ok(IndexLayoutStarknetStorageValue(
            Felt::deserialize(&mut &value.0[..]).map(StarknetStorageValue).ok_or(
                DeserializationError::ValueError(Box::new(IndexLayoutLeafDeserializationError(
                    "failed to deserialize StarknetStorageValue from index DB",
                ))),
            )?,
        ))
    }
}
