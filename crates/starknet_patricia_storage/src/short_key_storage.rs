use std::marker::PhantomData;

use blake2::digest::consts::{
    U16,
    U17,
    U18,
    U19,
    U20,
    U21,
    U22,
    U23,
    U24,
    U25,
    U26,
    U27,
    U28,
    U29,
    U30,
    U31,
    U32,
};
use blake2::Blake2s;
use digest::Digest;

use crate::storage_trait::{DbHashMap, DbKey, DbValue, PatriciaStorageResult, Storage};

#[macro_export]
macro_rules! define_short_key_storage {
    ($( ( $sizes:expr, $sizes_type:ty, $names:ident ) ),+ $(,)?) => {
        $(
            $crate::define_short_key_storage!($sizes, $sizes_type, $names);
        )+

        /// Wrap an existing storage implementation in a boxed short key storage implementation.
        /// If no size is given, boxes and returns the original storage.
        /// Panics if the size is not within the allowed range.
        pub fn wrap_storage_or_panic<S: Storage + 'static>(
            size: Option<u8>,
            storage: S,
        ) -> Box<dyn Storage> {
            let Some(size) = size else {
                return Box::new(storage);
            };
            match size {
                $( $sizes => Box::new($names::new(storage)), )+
                size => panic!("Invalid key size {size}."),
            }
        }
    };

    ($size:expr, $size_type:ty, $name:ident) => {
        /// A storage that hashes (using blake) each key to a $size - byte key.
        pub struct $name<S: Storage> {
            storage: S,
            _n_bytes: PhantomData<$size_type>,
        }

        impl<S: Storage> $name<S> {
            pub fn new(storage: S) -> Self {
                Self { storage, _n_bytes: PhantomData }
            }

            pub fn small_key(key: &DbKey) -> DbKey {
                let mut hasher = Blake2s::<$size_type>::new();
                hasher.update(key.0.as_slice());
                let result = hasher.finalize();
                DbKey(result.as_slice().to_vec())
            }
        }

        impl<S: Storage> Storage for $name<S> {
            fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
                self.storage.get(&Self::small_key(key))
            }

            fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<Option<DbValue>> {
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

            fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
                self.storage.delete(&Self::small_key(key))
            }

        }
    };
}

define_short_key_storage!(
    (16, U16, ShortKeyStorage16),
    (17, U17, ShortKeyStorage17),
    (18, U18, ShortKeyStorage18),
    (19, U19, ShortKeyStorage19),
    (20, U20, ShortKeyStorage20),
    (21, U21, ShortKeyStorage21),
    (22, U22, ShortKeyStorage22),
    (23, U23, ShortKeyStorage23),
    (24, U24, ShortKeyStorage24),
    (25, U25, ShortKeyStorage25),
    (26, U26, ShortKeyStorage26),
    (27, U27, ShortKeyStorage27),
    (28, U28, ShortKeyStorage28),
    (29, U29, ShortKeyStorage29),
    (30, U30, ShortKeyStorage30),
    (31, U31, ShortKeyStorage31),
    (32, U32, ShortKeyStorage32)
);
