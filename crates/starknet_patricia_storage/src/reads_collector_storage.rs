use crate::storage_trait::{
    DbHashMap,
    DbKey,
    DbValue,
    GatherableStorage,
    ImmutableReadOnlyStorage,
    NullStorage,
    PatriciaStorageResult,
    ReadOnlyStorage,
};

/// Wraps an [ImmutableReadOnlyStorage] reference and collects all reads performed through it.
/// The collected reads can be retrieved via [Self::into_reads].
pub struct ReadsCollectorStorage<'a, S: ImmutableReadOnlyStorage> {
    storage: &'a S,
    reads: DbHashMap,
}

impl<'a, S: ImmutableReadOnlyStorage> ReadsCollectorStorage<'a, S> {
    pub fn new(storage: &'a S) -> Self {
        Self { storage, reads: DbHashMap::new() }
    }

    /// Consumes the collector and returns all reads performed through it.
    pub fn into_reads(self) -> DbHashMap {
        self.reads
    }
}

impl<'a, S: ImmutableReadOnlyStorage> ReadOnlyStorage for ReadsCollectorStorage<'a, S> {
    async fn get_mut(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let value = self.storage.get(key).await?;
        if let Some(ref v) = value {
            self.reads.insert(key.clone(), v.clone());
        }
        Ok(value)
    }

    async fn mget_mut(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let values = self.storage.mget(keys).await?;
        for (key, value) in keys.iter().zip(values.iter()) {
            if let Some(v) = value {
                self.reads.insert((*key).clone(), v.clone());
            }
        }
        Ok(values)
    }

    /// `ReadsCollectorStorage` is only used as a wrapper to collect reads during concurrent tasks
    /// running with CacheStorage (to cache reads retroactively). Since we never use it as the
    /// underlying storage for concurrent tasks, we return `None` here.
    fn as_gatherable_storage(&mut self) -> Option<&mut impl GatherableStorage> {
        None::<&mut NullStorage>
    }
}
