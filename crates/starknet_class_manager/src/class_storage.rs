use starknet_api::class_cache::GlobalContractCache;
use starknet_class_manager_types::{ClassId, ClassStorageError, ExecutableClassHash};
use starknet_sierra_multicompile_types::{RawClass, RawExecutableClass};
use thiserror::Error;

// TODO(Elin): restrict visibility once this code is used.

pub type ClassStorageResult<T> = Result<T, ClassStorageError>;

pub trait ClassStorage: Send + Sync {
    type Error;

    fn set_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
        executable_class_hash: ExecutableClassHash,
        executable_class: RawExecutableClass,
    ) -> Result<(), Self::Error>;

    fn get_sierra(&self, class_id: ClassId) -> Result<RawClass, Self::Error>;

    fn get_executable(&self, class_id: ClassId) -> Result<RawExecutableClass, Self::Error>;

    fn get_executable_class_hash(
        &self,
        class_id: ClassId,
    ) -> Result<ExecutableClassHash, Self::Error>;

    fn set_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug)]
pub struct CachedClassStorageConfig {
    class_cache_size: usize,
    deprecated_class_cache_size: usize,
}

pub struct CachedClassStorage<S: ClassStorage> {
    storage: S,

    // Cache.
    classes: GlobalContractCache<RawClass>,
    executable_classes: GlobalContractCache<RawExecutableClass>,
    executable_class_hashes: GlobalContractCache<ExecutableClassHash>,
    deprecated_classes: GlobalContractCache<RawExecutableClass>,
}

#[derive(Debug, Error)]
pub enum CachedClassStorageError<E> {
    #[error("Class of hash: {class_id} not found")]
    ClassNotFound { class_id: ClassId },
    #[error(transparent)]
    StorageError(#[from] E),
}

impl<S: ClassStorage> CachedClassStorage<S> {
    pub fn new(config: CachedClassStorageConfig, storage: S) -> Self {
        Self {
            storage,
            classes: GlobalContractCache::new(config.class_cache_size),
            executable_classes: GlobalContractCache::new(config.class_cache_size),
            executable_class_hashes: GlobalContractCache::new(config.class_cache_size),
            deprecated_classes: GlobalContractCache::new(config.deprecated_class_cache_size),
        }
    }

    pub fn class_cached(&self, class_id: ClassId) -> bool {
        self.executable_class_hashes.get(&class_id).is_some()
    }

    pub fn deprecated_class_cached(&self, class_id: ClassId) -> bool {
        self.deprecated_classes.get(&class_id).is_some()
    }
}

impl<S: ClassStorage> ClassStorage for CachedClassStorage<S> {
    type Error = CachedClassStorageError<S::Error>;

    fn set_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
        executable_class_hash: ExecutableClassHash,
        executable_class: RawExecutableClass,
    ) -> Result<(), Self::Error> {
        if self.class_cached(class_id) {
            return Ok(());
        }

        self.storage.set_class(
            class_id,
            class.clone(),
            executable_class_hash,
            executable_class.clone(),
        )?;

        // Cache the class.
        // Done after successfully writing to storage as an optimization;
        // does not require atomicity.
        self.classes.set(class_id, class);
        self.executable_classes.set(class_id, executable_class);
        // Cache the executable class hash last; acts as an existence marker.
        self.executable_class_hashes.set(class_id, executable_class_hash);

        Ok(())
    }

    fn get_sierra(&self, class_id: ClassId) -> Result<RawClass, Self::Error> {
        if let Some(class) = self.classes.get(&class_id) {
            return Ok(class);
        }

        let class = self.storage.get_sierra(class_id)?;
        self.classes.set(class_id, class.clone());

        Ok(class)
    }

    fn get_executable(&self, class_id: ClassId) -> Result<RawExecutableClass, Self::Error> {
        if let Some(class) = self.deprecated_classes.get(&class_id) {
            return Ok(class);
        }

        let class = self.storage.get_executable(class_id)?;
        self.deprecated_classes.set(class_id, class.clone());

        Ok(class)
    }

    fn get_executable_class_hash(
        &self,
        class_id: ClassId,
    ) -> Result<ExecutableClassHash, Self::Error> {
        if let Some(class_hash) = self.executable_class_hashes.get(&class_id) {
            return Ok(class_hash);
        }

        let class_hash = self.storage.get_executable_class_hash(class_id)?;
        self.executable_class_hashes.set(class_id, class_hash);

        Ok(class_hash)
    }

    fn set_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> Result<(), Self::Error> {
        if self.deprecated_class_cached(class_id) {
            return Ok(());
        }

        self.storage.set_deprecated_class(class_id, class.clone())?;
        self.deprecated_classes.set(class_id, class);

        Ok(())
    }
}
