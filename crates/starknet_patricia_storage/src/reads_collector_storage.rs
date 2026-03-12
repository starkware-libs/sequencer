use std::sync::Arc;

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
pub struct ReadsCollectorStorage<S: ImmutableReadOnlyStorage> {
    pub storage: Arc<S>,
    reads: DbHashMap,
}

impl<S: ImmutableReadOnlyStorage> ReadsCollectorStorage<S> {
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage, reads: DbHashMap::new() }
    }

    /// Consumes the collector and returns all reads performed through it.
    pub fn into_reads(self) -> DbHashMap {
        self.reads
    }
}

impl<S: ImmutableReadOnlyStorage> ReadOnlyStorage for ReadsCollectorStorage<S> {
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
}
