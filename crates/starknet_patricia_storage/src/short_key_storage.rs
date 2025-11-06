use std::marker::PhantomData;

use blake2::Blake2s;
use digest::Digest;

use crate::storage_trait::{DbHashMap, DbKey, DbValue, PatriciaStorageResult, Storage};

#[macro_export]
macro_rules! define_short_key_storage {
    ($( ( $sizes:ident, $names:ident ) ),+ $(,)?) => {
        $(
            $crate::define_short_key_storage!($sizes, $names);
        )+

        /// Utility enum to define the possible sizes of the short key storage.
        pub enum ShortKeySize {
            $($sizes,)+
        }
    };

    ($size:ident, $name:ident) => {
        /// A storage that hashes (using blake) each key to a $size - byte key.
        pub struct $name<S: Storage> {
            storage: S,
            _n_bytes: PhantomData<blake2::digest::consts::$size>,
        }

        impl<S: Storage> $name<S> {
            pub fn new(storage: S) -> Self {
                Self { storage, _n_bytes: PhantomData }
            }

            pub fn small_key(key: &DbKey) -> DbKey {
                let mut hasher = Blake2s::<blake2::digest::consts::$size>::new();
                hasher.update(key.0.as_slice());
                let result = hasher.finalize();
                DbKey(result.as_slice().to_vec())
            }
        }

        impl<S: Storage> Storage for $name<S> {
            type Stats = S::Stats;

            fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
                self.storage.get(&Self::small_key(key))
            }

            fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
                self.storage.set(Self::small_key(&key), value)
            }

            fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
                let small_keys = keys
                    .iter()
                    .map(|key| Self::small_key(key))
                    .collect::<Vec<_>>();
                self.storage.mget(small_keys.iter().collect::<Vec<&DbKey>>().as_slice())
            }

            fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
                self.storage.mset(
                    key_to_value
                        .into_iter()
                        .map(|(key, value)| (Self::small_key(&key), value))
                        .collect()
                )
            }

            fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
                self.storage.delete(&Self::small_key(key))
            }

            fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
                self.storage.get_stats()
            }
        }
    };
}

define_short_key_storage!(
    (U16, ShortKeyStorage16),
    (U17, ShortKeyStorage17),
    (U18, ShortKeyStorage18),
    (U19, ShortKeyStorage19),
    (U20, ShortKeyStorage20),
    (U21, ShortKeyStorage21),
    (U22, ShortKeyStorage22),
    (U23, ShortKeyStorage23),
    (U24, ShortKeyStorage24),
    (U25, ShortKeyStorage25),
    (U26, ShortKeyStorage26),
    (U27, ShortKeyStorage27),
    (U28, ShortKeyStorage28),
    (U29, ShortKeyStorage29),
    (U30, ShortKeyStorage30),
    (U31, ShortKeyStorage31),
    (U32, ShortKeyStorage32)
);
