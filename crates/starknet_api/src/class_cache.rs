use std::sync::{Arc, Mutex, MutexGuard};

use cached::{Cached, SizedCache};

use crate::core::ClassHash;

type ContractLRUCache<T> = SizedCache<ClassHash, T>;
type LockedClassCache<'a, T> = MutexGuard<'a, ContractLRUCache<T>>;

// TODO(Yoni, 1/2/2025): consider defining CachedStateReader.
/// Thread-safe LRU cache for contract classes (Sierra or compiled Casm/Native), optimized for
/// inter-language sharing when `blockifier` compiles as a shared library.
#[derive(Clone, Debug)]
pub struct GlobalContractCache<T: Clone>(pub Arc<Mutex<ContractLRUCache<T>>>);

impl<T: Clone> GlobalContractCache<T> {
    /// Locks the cache for atomic access. Although conceptually shared, writing to this cache is
    /// only possible for one writer at a time.
    pub fn lock(&self) -> LockedClassCache<'_, T> {
        self.0.lock().expect("Global contract cache is poisoned.")
    }

    pub fn get(&self, class_hash: &ClassHash) -> Option<T> {
        self.lock().cache_get(class_hash).cloned()
    }

    pub fn set(&self, class_hash: ClassHash, contract_class: T) {
        self.lock().cache_set(class_hash, contract_class);
    }

    pub fn clear(&mut self) {
        self.lock().cache_clear();
    }

    pub fn new(cache_size: usize) -> Self {
        Self(Arc::new(Mutex::new(ContractLRUCache::<T>::with_size(cache_size))))
    }
}
