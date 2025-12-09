use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::errors::LeafResult;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia_storage::db_object::{
    DBObject,
    EmptyDeserializationContext,
    HasStaticPrefix,
};
use starknet_patricia_storage::errors::{DeserializationError, SerializationError};
use starknet_patricia_storage::storage_trait::{DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

// Wrap the leaves types so that we can implement the [DBObject] trait differently in index
// layout.
#[derive(Clone, Debug, Default, Eq, PartialEq, derive_more::AsRef, derive_more::From)]
pub struct IndexLayoutContractState(pub ContractState);

#[derive(Clone, Debug, Default, Eq, PartialEq, derive_more::AsRef, derive_more::From)]
pub struct IndexLayoutCompiledClassHash(pub CompiledClassHash);

#[derive(Clone, Debug, Default, Eq, PartialEq, derive_more::From)]
pub struct IndexLayoutStarknetStorageValue(pub StarknetStorageValue);

// TODO(Ariel): Delete this enum and use `CommitmentType` instead.
#[derive(Debug, PartialEq)]
pub enum TrieType {
    ContractsTrie,
    ClassesTrie,
    StorageTrie(ContractAddress),
}

impl TrieType {
    fn db_prefix(&self) -> DbKeyPrefix {
        match self {
            TrieType::ContractsTrie => DbKeyPrefix::new(b"CONTRACTS_TREE_PREFIX".into()),
            TrieType::ClassesTrie => DbKeyPrefix::new(b"CLASSES_TREE_PREFIX".into()),
            TrieType::StorageTrie(contract_address) => {
                let prefix = contract_address.to_bytes_be().to_vec();
                DbKeyPrefix::new(prefix.into())
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
                    key_context.db_prefix()
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
                    let (created_leaf, output) = <$inner as Leaf>::create(input).await?;
                    Ok((Self(created_leaf), output))
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

impl DBObject for IndexLayoutContractState {
    type DeserializeContext = EmptyDeserializationContext;
    fn serialize(&self) -> Result<DbValue, SerializationError> {
        serialize_felts(vec![self.0.class_hash.0, self.0.storage_root_hash.0, self.0.nonce.0])
    }

    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        let mut cursor: &[u8] = &value.0;
        let class_hash = deserialize_felt(&mut cursor, value)?;
        let storage_root_hash = deserialize_felt(&mut cursor, value)?;
        let nonce = deserialize_felt(&mut cursor, value)?;
        Ok(IndexLayoutContractState(ContractState {
            class_hash: ClassHash(class_hash),
            storage_root_hash: HashOutput(storage_root_hash),
            nonce: Nonce(nonce),
        }))
    }
}

impl DBObject for IndexLayoutCompiledClassHash {
    type DeserializeContext = EmptyDeserializationContext;
    fn serialize(&self) -> Result<DbValue, SerializationError> {
        serialize_felts(vec![self.0.0])
    }
    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        Ok(IndexLayoutCompiledClassHash(CompiledClassHash(deserialize_felt(
            &mut &value.0[..],
            value,
        )?)))
    }
}

impl DBObject for IndexLayoutStarknetStorageValue {
    type DeserializeContext = EmptyDeserializationContext;
    fn serialize(&self) -> Result<DbValue, SerializationError> {
        serialize_felts(vec![self.0.0])
    }
    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        Ok(IndexLayoutStarknetStorageValue(StarknetStorageValue(deserialize_felt(
            &mut &value.0[..],
            value,
        )?)))
    }
}

fn deserialize_felt(cursor: &mut &[u8], value: &DbValue) -> Result<Felt, DeserializationError> {
    Felt::deserialize(cursor).ok_or(DeserializationError::FeltDeserializationError(value.clone()))
}

fn serialize_felts(felts: Vec<Felt>) -> Result<DbValue, SerializationError> {
    let mut buffer = Vec::new();
    for felt in felts {
        felt.serialize(&mut buffer)?;
    }
    Ok(DbValue(buffer))
}

#[cfg(test)]
mod leaves_serde_tests {
    use starknet_api::core::{ClassHash, Nonce};
    use starknet_api::hash::HashOutput;
    use starknet_patricia_storage::db_object::{DBObject, EmptyDeserializationContext};
    use starknet_types_core::felt::Felt;

    use crate::block_committer::input::StarknetStorageValue;
    use crate::db::index_db::leaves::{
        IndexLayoutCompiledClassHash,
        IndexLayoutContractState,
        IndexLayoutStarknetStorageValue,
    };
    use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
    use crate::patricia_merkle_tree::types::CompiledClassHash;

    #[test]
    fn test_index_layout_contract_state_serde() {
        let contract_state = ContractState {
            class_hash: ClassHash(Felt::from(1)),
            storage_root_hash: HashOutput(Felt::from(2)),
            nonce: Nonce(Felt::from(3)),
        };
        let index_layout_contract_state = IndexLayoutContractState(contract_state);
        let serialized = index_layout_contract_state.serialize().unwrap();
        let deserialized =
            IndexLayoutContractState::deserialize(&serialized, &EmptyDeserializationContext)
                .unwrap();
        assert_eq!(index_layout_contract_state, deserialized);
    }

    #[test]
    fn test_index_layout_compiled_class_hash_serde() {
        let compiled_class_hash = CompiledClassHash(Felt::from(1));
        let index_layout_compiled_class_hash = IndexLayoutCompiledClassHash(compiled_class_hash);
        let serialized = index_layout_compiled_class_hash.serialize().unwrap();
        let deserialized =
            IndexLayoutCompiledClassHash::deserialize(&serialized, &EmptyDeserializationContext)
                .unwrap();
        assert_eq!(index_layout_compiled_class_hash, deserialized);
    }

    #[test]
    fn test_index_layout_starknet_storage_value_serde() {
        let starknet_storage_value = StarknetStorageValue(Felt::from(1));
        let index_layout_starknet_storage_value =
            IndexLayoutStarknetStorageValue(starknet_storage_value);
        let serialized = index_layout_starknet_storage_value.serialize().unwrap();
        let deserialized =
            IndexLayoutStarknetStorageValue::deserialize(&serialized, &EmptyDeserializationContext)
                .unwrap();
        assert_eq!(index_layout_starknet_storage_value, deserialized);
    }
}
