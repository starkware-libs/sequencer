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
    let overlay_val = DbValue(vec![99]);
    two.overlay.set(key.clone(), overlay_val.clone()).await.unwrap();
    assert_eq!(two.get_mut(&key).await.unwrap(), Some(overlay_val));
    two.overlay.delete(&key).await.unwrap();
    assert_eq!(two.get_mut(&key).await.unwrap(), Some(base_val));
}

#[tokio::test]
async fn mget_mut_uses_immutable_base_mget_on_miss() {
    let key_base_only = DbKey(vec![3]);
    let key_overlay = DbKey(vec![4]);
    let base_val = DbValue(vec![11]);
    let over_val = DbValue(vec![22]);
    let mut base = MapStorage::default();
    base.0.insert(key_base_only.clone(), base_val.clone());

    let mut layered = TwoLayerStorage::new(MapStorage::default(), &base);
    layered.overlay.set(key_overlay.clone(), over_val.clone()).await.unwrap();

    let keys = [&key_base_only, &key_overlay];
    assert_eq!(layered.mget_mut(&keys).await.unwrap(), vec![Some(base_val), Some(over_val)]);
}
