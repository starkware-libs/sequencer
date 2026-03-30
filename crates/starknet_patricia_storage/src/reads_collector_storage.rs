use crate::storage_trait::{
    DbHashMap,
    DbKey,
    DbValue,
    ImmutableReadOnlyStorage,
    PatriciaStorageResult,
    ReadOnlyStorage,
};

/// Wraps an [ImmutableReadOnlyStorage] reference and collects all reads performed through it.
/// The collected reads can be retrieved via [Self::into_reads].
pub struct ReadsCollectorStorage<'a, S: ImmutableReadOnlyStorage + ?Sized> {
    storage: &'a S,
    reads: DbHashMap,
}

impl<'a, S: ImmutableReadOnlyStorage + ?Sized> ReadsCollectorStorage<'a, S> {
    pub fn new(storage: &'a S) -> Self {
        Self { storage, reads: DbHashMap::new() }
    }

    /// Consumes the collector and returns all reads performed through it.
    pub fn into_reads(self) -> DbHashMap {
        self.reads
    }
}

impl<'a, S: ImmutableReadOnlyStorage + ?Sized> ReadOnlyStorage for ReadsCollectorStorage<'a, S> {
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
}
