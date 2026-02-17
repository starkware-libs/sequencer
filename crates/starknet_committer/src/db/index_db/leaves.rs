use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::errors::LeafResult;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia_storage::db_object::{
    DBObject,
    EmptyDeserializationContext,
    EmptyKeyContext,
    HasStaticPrefix,
};
use starknet_patricia_storage::errors::{DeserializationError, SerializationError};
use starknet_patricia_storage::storage_trait::{DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

use crate::block_committer::input::StarknetStorageValue;
use crate::db::index_db::{CLASSES_TREE_PREFIX, CONTRACTS_TREE_PREFIX};
use crate::db::mock_forest_storage::EMPTY_DB_KEY_SEPARATOR;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub const INDEX_LAYOUT_DB_KEY_SEPARATOR: &[u8] = EMPTY_DB_KEY_SEPARATOR;

// Wrap the leaves types so that we can implement the [DBObject] trait differently in index
// layout.
#[derive(
    Clone, Debug, Default, Eq, PartialEq, derive_more::AsRef, derive_more::From, derive_more::Into,
)]
pub struct IndexLayoutContractState(pub ContractState);

#[derive(
    Clone, Debug, Default, Eq, PartialEq, derive_more::AsRef, derive_more::From, derive_more::Into,
)]
pub struct IndexLayoutCompiledClassHash(pub CompiledClassHash);

#[derive(Clone, Debug, Default, Eq, PartialEq, derive_more::From, derive_more::Into)]
pub struct IndexLayoutStarknetStorageValue(pub StarknetStorageValue);

macro_rules! impl_has_static_prefix_empty_context {
    ($($ty:ty => $prefix:expr),* $(,)?) => {
        $(
            impl HasStaticPrefix for $ty {
                type KeyContext = EmptyKeyContext;
                fn get_static_prefix(_key_context: &Self::KeyContext) -> DbKeyPrefix {
                    DbKeyPrefix::new((&$prefix[..]).into())
                }
            }
        )*
    };
}

impl_has_static_prefix_empty_context! {
    IndexLayoutContractState => CONTRACTS_TREE_PREFIX,
    IndexLayoutCompiledClassHash => CLASSES_TREE_PREFIX,
}

impl HasStaticPrefix for IndexLayoutStarknetStorageValue {
    type KeyContext = ContractAddress;

    /// Returns the contract address, which is the storage trie prefix in index layout.
    fn get_static_prefix(key_context: &Self::KeyContext) -> DbKeyPrefix {
        let prefix = key_context.to_bytes_be().to_vec();
        DbKeyPrefix::new(prefix.into())
    }
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
    const DB_KEY_SEPARATOR: &[u8] = INDEX_LAYOUT_DB_KEY_SEPARATOR;

    type DeserializeContext = EmptyDeserializationContext;

    fn serialize(&self) -> Result<DbValue, SerializationError> {
        serialize_felts(&[self.0.class_hash.0, self.0.storage_root_hash.0, self.0.nonce.0])
    }

    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        let mut cursor: &[u8] = &value.0;
        let err = || DeserializationError::FeltDeserialization(value.clone());

        let class_hash = deserialize_felt(&mut cursor, err)?;
        let storage_root_hash = deserialize_felt(&mut cursor, err)?;
        let nonce = deserialize_felt(&mut cursor, err)?;

        Ok(Self(ContractState {
            class_hash: ClassHash(class_hash),
            storage_root_hash: HashOutput(storage_root_hash),
            nonce: Nonce(nonce),
        }))
    }
}

impl DBObject for IndexLayoutCompiledClassHash {
    const DB_KEY_SEPARATOR: &[u8] = INDEX_LAYOUT_DB_KEY_SEPARATOR;

    type DeserializeContext = EmptyDeserializationContext;

    fn serialize(&self) -> Result<DbValue, SerializationError> {
        serialize_felts(&[self.0.0])
    }

    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        Ok(Self(CompiledClassHash(deserialize_felt(&mut &value.0[..], || {
            DeserializationError::FeltDeserialization(value.clone())
        })?)))
    }
}

impl DBObject for IndexLayoutStarknetStorageValue {
    const DB_KEY_SEPARATOR: &[u8] = INDEX_LAYOUT_DB_KEY_SEPARATOR;

    type DeserializeContext = EmptyDeserializationContext;

    fn serialize(&self) -> Result<DbValue, SerializationError> {
        serialize_felts(&[self.0.0])
    }

    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        Ok(Self(StarknetStorageValue(deserialize_felt(&mut &value.0[..], || {
            DeserializationError::FeltDeserialization(value.clone())
        })?)))
    }
}

fn deserialize_felt(
    cursor: &mut &[u8],
    mk_err: impl Fn() -> DeserializationError,
) -> Result<Felt, DeserializationError> {
    Felt::deserialize(cursor).ok_or_else(mk_err)
}

fn serialize_felts(felts: &[Felt]) -> Result<DbValue, SerializationError> {
    let mut buffer = Vec::new();
    for felt in felts {
        felt.serialize(&mut buffer)?;
    }
    Ok(DbValue(buffer))
}
