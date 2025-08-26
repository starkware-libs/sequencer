use std::collections::HashMap;

use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, Storage};

<<<<<<< HEAD
pub type MapStorage = HashMap<DbKey, DbValue>;

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
||||||| 01792faa8
#[derive(Serialize, Debug, Default)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct MapStorage {
    pub storage: HashMap<DbKey, DbValue>,
=======
pub type MapStorage = HashMap<DbKey, DbValue>;

#[derive(Serialize, Debug)]
pub struct BorrowedMapStorage<'a> {
    pub storage: &'a mut MapStorage,
>>>>>>> origin/main-v0.14.1
}

<<<<<<< HEAD
impl Storage for MapStorage {
||||||| 01792faa8
impl Storage for MapStorage {
    fn get(&self, key: &DbKey) -> Option<&DbValue> {
        self.storage.get(key)
    }

=======
impl Storage for BorrowedMapStorage<'_> {
>>>>>>> origin/main-v0.14.1
    fn set(&mut self, key: DbKey, value: DbValue) -> Option<DbValue> {
        self.insert(key, value)
    }

<<<<<<< HEAD
    fn mset(&mut self, key_to_value: MapStorage) {
        self.extend(key_to_value);
||||||| 01792faa8
    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        keys.iter().map(|key| self.get(key)).collect::<Vec<_>>()
    }

    fn mset(&mut self, key_to_value: HashMap<DbKey, DbValue>) {
        self.storage.extend(key_to_value);
=======
    fn mset(&mut self, key_to_value: MapStorage) {
        self.storage.extend(key_to_value);
>>>>>>> origin/main-v0.14.1
    }

    fn delete(&mut self, key: &DbKey) -> Option<DbValue> {
        self.remove(key)
    }

<<<<<<< HEAD
    fn get(&self, key: &DbKey) -> Option<&DbValue> {
        self.get(key)
    }

    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        keys.iter().map(|key| self.get(key)).collect()
||||||| 01792faa8
impl From<HashMap<DbKey, DbValue>> for MapStorage {
    fn from(storage: HashMap<DbKey, DbValue>) -> Self {
        Self { storage }
=======
    fn get(&self, key: &DbKey) -> Option<&DbValue> {
        self.storage.get(key)
    }

    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        keys.iter().map(|key| self.storage.get(key)).collect()
>>>>>>> origin/main-v0.14.1
    }
}
