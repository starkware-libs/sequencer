use crate::storage_trait::{
    DbHashMap,
    DbKey,
    DbValue,
    ImmutableReadOnlyStorage,
    NullStorage,
    PatriciaStorageResult,
    ReadOnlyStorage,
};

/// A collection of key-value pairs read from storage.
/// Used to accumulate reads across concurrent tasks and merge them back into a single storage.
/// It's important that the inner map can only be modified via private methods in this module,
/// otherwise, [Storage::handle_collected_reads] can be dangerous and allow setting arbitrary data
/// in the storage.
#[derive(Clone, Default)]
pub struct StorageReads(DbHashMap);

impl StorageReads {
    pub fn new() -> Self {
        Self(DbHashMap::new())
    }

    // This method must be private, see struct documentation.
    fn insert(&mut self, key: DbKey, value: DbValue) {
        self.0.insert(key, value);
    }

    pub fn extend(&mut self, other: StorageReads) {
        self.0.extend(other.0);
    }

    pub fn into_inner(self) -> DbHashMap {
        self.0
    }
}

/// Wraps an [ImmutableReadOnlyStorage] reference and collects all reads performed through it.
/// The collected reads can be retrieved via [ReadsCollectorStorage::into_reads].
pub struct ReadsCollectorStorage<'a, S: ImmutableReadOnlyStorage> {
    pub storage: &'a S,
    pub reads: StorageReads,
}

impl<'a, S: ImmutableReadOnlyStorage> ReadsCollectorStorage<'a, S> {
    pub fn new(storage: &'a S) -> Self {
        Self { storage, reads: StorageReads::new() }
    }

    /// Consumes the collector and returns all reads performed through it.
    pub fn into_reads(self) -> StorageReads {
        self.reads
    }
}

impl<'a, S: ImmutableReadOnlyStorage> ReadOnlyStorage for ReadsCollectorStorage<'a, S> {
    async fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let value = self.storage.get(key).await?;
        if let Some(ref v) = value {
            self.reads.insert(key.clone(), v.clone());
        }
        Ok(value)
    }

    async fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let values = self.storage.mget(keys).await?;
        for (key, value) in keys.iter().zip(values.iter()) {
            if let Some(v) = value {
                self.reads.insert((*key).clone(), v.clone());
            }
        }
        Ok(values)
    }

    fn get_immutable_read_only_self(&self) -> Option<&impl ImmutableReadOnlyStorage> {
        None::<&NullStorage>
    }
}
