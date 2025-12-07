use std::collections::HashMap;
use std::hash::Hash;

use crate::map_storage::MapStorage;
use crate::storage_trait::{DbHashMap, PatriciaStorageResult, Storage};

pub type FamilyName = String;

pub trait Family: Clone + Eq + Hash {
    fn name(&self) -> FamilyName;
}

#[derive(Default)]
pub struct FamilyStorage<F: Family> {
    family_map: HashMap<F, MapStorage>,
}

impl<F: Family> FamilyStorage<F> {
    pub fn get_storage(&mut self, family: F) -> &mut MapStorage {
        if !self.family_map.contains_key(&family) {
            self.family_map.insert(family.clone(), MapStorage(HashMap::new()));
        }
        self.family_map
            .get_mut(&family)
            .unwrap_or_else(|| panic!("Family {} not found", family.name()))
    }

    pub async fn mset_to_storage(
        &mut self,
        family: F,
        key_to_value: DbHashMap,
    ) -> PatriciaStorageResult<()> {
        let family_storage = self.get_storage(family);
        family_storage.mset(key_to_value).await
    }
}

pub trait SetFamilyStorage<F: Family> {
    fn mset_family_storage(
        &mut self,
        family_storage: FamilyStorage<F>,
    ) -> PatriciaStorageResult<()>
    where
        Self: Storage;
}
