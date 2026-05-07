use crate::storage_trait::{
    DbKey,
    DbValue,
    ImmutableReadOnlyStorage,
    PatriciaStorageResult,
    ReadOnlyStorage,
};

#[cfg(test)]
#[path = "two_layer_storage_test.rs"]
mod two_layer_storage_test;
/// Overlay reads on top of a borrowed base [`ImmutableReadOnlyStorage`]: `overlay` is consulted
/// first via [`ImmutableReadOnlyStorage::get`] / [`ImmutableReadOnlyStorage::mget`]; on miss,
/// reads relay to `base`.
///
/// [`ReadOnlyStorage::get_mut`] / [`ReadOnlyStorage::mget_mut`] use the same immutable overlay and
/// base paths on overlay misses so the composite implements [`ReadOnlyStorage`] while holding
/// `&'a Base`. Patricia paths reads do not mutate the underlying storage.
/// This allows passing `TwoLayerStorage` to `fetch_all_patricia_paths`, which requires `&mut`
/// [`ReadOnlyStorage`].
pub struct TwoLayerStorage<'a, Overlay, Base>
where
    Overlay: ImmutableReadOnlyStorage + Sync,
    Base: ImmutableReadOnlyStorage + Sync + ?Sized,
{
    overlay: Overlay,
    base: &'a Base,
}

impl<'a, Overlay, Base> TwoLayerStorage<'a, Overlay, Base>
where
    Overlay: ImmutableReadOnlyStorage + Sync,
    Base: ImmutableReadOnlyStorage + Sync + ?Sized,
{
    pub fn new(overlay: Overlay, base: &'a Base) -> Self {
        Self { overlay, base }
    }
}

impl<'a, Overlay, Base> ReadOnlyStorage for TwoLayerStorage<'a, Overlay, Base>
where
    Overlay: ImmutableReadOnlyStorage + Sync,
    Base: ImmutableReadOnlyStorage + Sync + ?Sized,
{
    async fn get_mut(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(match self.overlay.get(key).await? {
            Some(v) => Some(v),
            None => self.base.get(key).await?,
        })
    }

    async fn mget_mut(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let mut out = self.overlay.mget(keys).await?;
        let mut miss_indices = Vec::new();
        let mut miss_keys = Vec::new();
        for (i, v) in out.iter().enumerate() {
            if v.is_none() {
                miss_indices.push(i);
                miss_keys.push(keys[i]);
            }
        }
        if !miss_keys.is_empty() {
            let fetched = self.base.mget(&miss_keys).await?;
            for (idx, val) in miss_indices.into_iter().zip(fetched) {
                out[idx] = val;
            }
        }
        Ok(out)
    }
}
