use crate::storage_trait::{
    DbKey,
    DbValue,
    ImmutableReadOnlyStorage,
    PatriciaStorageResult,
    ReadOnlyStorage,
};

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
    pub overlay: Overlay,
    pub base: &'a Base,
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

#[cfg(test)]
mod tests {
    use super::TwoLayerStorage;
    use crate::map_storage::MapStorage;
    use crate::storage_trait::{DbKey, DbValue, ReadOnlyStorage, Storage};

    #[tokio::test]
    async fn read_falls_through_to_base() {
        let key = DbKey(vec![1, 2, 3]);
        let val = DbValue(vec![9]);
        let mut base = MapStorage::default();
        base.0.insert(key.clone(), val.clone());

        let mut two = TwoLayerStorage::new(MapStorage::default(), &base);
        assert_eq!(two.get_mut(&key).await.unwrap(), Some(val));
    }

    #[tokio::test]
    async fn overlay_shadows_base() {
        let key = DbKey(vec![1]);
        let base_val = DbValue(vec![1]);
        let over_val = DbValue(vec![2]);
        let mut base = MapStorage::default();
        base.0.insert(key.clone(), base_val);

        let mut two = TwoLayerStorage::new(MapStorage::default(), &base);
        two.overlay.set(key.clone(), over_val.clone()).await.unwrap();
        assert_eq!(two.get_mut(&key).await.unwrap(), Some(over_val));
    }

    #[tokio::test]
    async fn delete_drops_overlay_entry_and_sees_base() {
        let key = DbKey(vec![7]);
        let base_val = DbValue(vec![42]);
        let mut base = MapStorage::default();
        base.0.insert(key.clone(), base_val.clone());

        let mut two = TwoLayerStorage::new(MapStorage::default(), &base);
        two.overlay.set(key.clone(), DbValue(vec![99])).await.unwrap();
        two.overlay.delete(&key).await.unwrap();
        assert_eq!(two.get_mut(&key).await.unwrap(), Some(base_val));
    }

    #[tokio::test]
    async fn mget_mut_uses_immutable_base_mget_on_miss() {
        let key = DbKey(vec![3]);
        let val = DbValue(vec![11]);
        let mut base = MapStorage::default();
        base.0.insert(key.clone(), val.clone());

        let mut layered = TwoLayerStorage::new(MapStorage::default(), &base);
        assert_eq!(layered.get_mut(&key).await.unwrap(), Some(val));
    }
}
