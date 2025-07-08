use std::collections::BTreeMap;
use std::error::Error;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use apollo_class_manager_types::{CachedClassStorageError, ClassId, ExecutableClassHash};
use apollo_compile_to_casm_types::{RawClass, RawClassError, RawExecutableClass};
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_storage::class_hash::{ClassHashStorageReader, ClassHashStorageWriter};
use apollo_storage::StorageConfig;
use serde::{Deserialize, Serialize};
use starknet_api::class_cache::GlobalContractCache;
use starknet_api::contract_class::ContractClass;
use thiserror::Error;
use tracing::instrument;

use crate::config::{ClassHashStorageConfig, FsClassStorageConfig};
use crate::metrics::{increment_n_classes, record_class_size, CairoClassType, ClassObjectType};

#[cfg(test)]
#[path = "class_storage_test.rs"]
mod class_storage_test;

// TODO(Elin): restrict visibility once this code is used.

pub trait ClassStorage: Send + Sync {
    type Error: Error;

    fn set_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
        executable_class_hash: ExecutableClassHash,
        executable_class: RawExecutableClass,
    ) -> Result<(), Self::Error>;

    fn get_sierra(&self, class_id: ClassId) -> Result<Option<RawClass>, Self::Error>;

    fn get_executable(&self, class_id: ClassId) -> Result<Option<RawExecutableClass>, Self::Error>;

    fn get_executable_class_hash(
        &self,
        class_id: ClassId,
    ) -> Result<Option<ExecutableClassHash>, Self::Error>;

    fn set_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> Result<(), Self::Error>;

    fn get_deprecated_class(
        &self,
        class_id: ClassId,
    ) -> Result<Option<RawExecutableClass>, Self::Error>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CachedClassStorageConfig {
    pub class_cache_size: usize,
    pub deprecated_class_cache_size: usize,
}

// TODO(Elin): provide default values for the fields.
impl Default for CachedClassStorageConfig {
    fn default() -> Self {
        Self { class_cache_size: 10, deprecated_class_cache_size: 10 }
    }
}

impl SerializeConfig for CachedClassStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "class_cache_size",
                &self.class_cache_size,
                "Contract classes cache size.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "deprecated_class_cache_size",
                &self.deprecated_class_cache_size,
                "Deprecated contract classes cache size.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

pub struct CachedClassStorage<S: ClassStorage> {
    storage: S,

    // Cache.
    classes: GlobalContractCache<RawClass>,
    executable_classes: GlobalContractCache<RawExecutableClass>,
    executable_class_hashes_v2: GlobalContractCache<ExecutableClassHash>,
    deprecated_classes: GlobalContractCache<RawExecutableClass>,
}

impl<S: ClassStorage> CachedClassStorage<S> {
    pub fn new(config: CachedClassStorageConfig, storage: S) -> Self {
        Self {
            storage,
            classes: GlobalContractCache::new(config.class_cache_size),
            executable_classes: GlobalContractCache::new(config.class_cache_size),
            executable_class_hashes_v2: GlobalContractCache::new(config.class_cache_size),
            deprecated_classes: GlobalContractCache::new(config.deprecated_class_cache_size),
        }
    }

    pub fn class_cached(&self, class_id: ClassId) -> bool {
        self.executable_class_hashes_v2.get(&class_id).is_some()
    }

    pub fn deprecated_class_cached(&self, class_id: ClassId) -> bool {
        self.deprecated_classes.get(&class_id).is_some()
    }
}

impl<S: ClassStorage> ClassStorage for CachedClassStorage<S> {
    type Error = CachedClassStorageError<S::Error>;

    #[instrument(skip(self, class, executable_class), level = "debug", ret, err)]
    fn set_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
        executable_class_hash_v2: ExecutableClassHash,
        executable_class: RawExecutableClass,
    ) -> Result<(), Self::Error> {
        if self.class_cached(class_id) {
            return Ok(());
        }

        self.storage.set_class(
            class_id,
            class.clone(),
            executable_class_hash_v2,
            executable_class.clone(),
        )?;

        increment_n_classes(CairoClassType::Regular);
        record_class_size(ClassObjectType::Sierra, &class);
        record_class_size(ClassObjectType::Casm, &executable_class);

        // Cache the class.
        // Done after successfully writing to storage as an optimization;
        // does not require atomicity.
        self.classes.set(class_id, class);
        self.executable_classes.set(class_id, executable_class);
        // Cache the executable class hash last; acts as an existence marker.
        self.executable_class_hashes_v2.set(class_id, executable_class_hash_v2);

        Ok(())
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_sierra(&self, class_id: ClassId) -> Result<Option<RawClass>, Self::Error> {
        if let Some(class) = self.classes.get(&class_id) {
            return Ok(Some(class));
        }

        let Some(class) = self.storage.get_sierra(class_id)? else {
            return Ok(None);
        };

        self.classes.set(class_id, class.clone());
        Ok(Some(class))
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_executable(&self, class_id: ClassId) -> Result<Option<RawExecutableClass>, Self::Error> {
        if let Some(class) = self
            .executable_classes
            .get(&class_id)
            .or_else(|| self.deprecated_classes.get(&class_id))
        {
            return Ok(Some(class));
        }

        let Some(class) = self.storage.get_executable(class_id)? else {
            return Ok(None);
        };

        // TODO(Elin): separate Cairo0<>1 getters to avoid deserializing here.
        match ContractClass::try_from(class.clone()).unwrap() {
            ContractClass::V0(_) => {
                self.deprecated_classes.set(class_id, class.clone());
            }
            ContractClass::V1(_) => {
                self.executable_classes.set(class_id, class.clone());
            }
        }

        Ok(Some(class))
    }

    #[instrument(skip(self), level = "debug", ret, err)]
    fn get_executable_class_hash(
        &self,
        class_id: ClassId,
    ) -> Result<Option<ExecutableClassHash>, Self::Error> {
        if let Some(class_hash) = self.executable_class_hashes_v2.get(&class_id) {
            return Ok(Some(class_hash));
        }

        let Some(class_hash) = self.storage.get_executable_class_hash(class_id)? else {
            return Ok(None);
        };

        self.executable_class_hashes_v2.set(class_id, class_hash);
        Ok(Some(class_hash))
    }

    #[instrument(skip(self, class), level = "debug", ret, err)]
    fn set_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> Result<(), Self::Error> {
        if self.deprecated_class_cached(class_id) {
            return Ok(());
        }

        self.storage.set_deprecated_class(class_id, class.clone())?;

        increment_n_classes(CairoClassType::Deprecated);
        record_class_size(ClassObjectType::DeprecatedCasm, &class);

        self.deprecated_classes.set(class_id, class);

        Ok(())
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_deprecated_class(
        &self,
        class_id: ClassId,
    ) -> Result<Option<RawExecutableClass>, Self::Error> {
        if let Some(class) = self.deprecated_classes.get(&class_id) {
            return Ok(Some(class));
        }

        let Some(class) = self.storage.get_deprecated_class(class_id)? else {
            return Ok(None);
        };

        self.deprecated_classes.set(class_id, class.clone());
        Ok(Some(class))
    }
}

impl Clone for CachedClassStorage<FsClassStorage> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            classes: self.classes.clone(),
            executable_classes: self.executable_classes.clone(),
            executable_class_hashes_v2: self.executable_class_hashes_v2.clone(),
            deprecated_classes: self.deprecated_classes.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ClassHashStorageError {
    #[error(transparent)]
    Storage(#[from] apollo_storage::StorageError),
}

type ClassHashStorageResult<T> = Result<T, ClassHashStorageError>;
type LockedWriter<'a> = MutexGuard<'a, apollo_storage::StorageWriter>;

#[derive(Clone)]
pub struct ClassHashStorage {
    reader: apollo_storage::StorageReader,
    writer: Arc<Mutex<apollo_storage::StorageWriter>>,
}

impl ClassHashStorage {
    pub fn new(config: ClassHashStorageConfig) -> ClassHashStorageResult<Self> {
        let storage_config = StorageConfig::from(config);
        let (reader, writer) = apollo_storage::open_storage(storage_config)?;

        Ok(Self { reader, writer: Arc::new(Mutex::new(writer)) })
    }

    fn writer(&self) -> ClassHashStorageResult<LockedWriter<'_>> {
        Ok(self.writer.lock().expect("Writer is poisoned."))
    }

    #[instrument(skip(self), level = "debug", ret, err)]
    fn get_executable_class_hash(
        &self,
        class_id: ClassId,
    ) -> ClassHashStorageResult<Option<ExecutableClassHash>> {
        Ok(self.reader.begin_ro_txn()?.get_executable_class_hash_v2(&class_id)?)
    }

    #[instrument(skip(self), level = "debug", ret, err)]
    fn set_executable_class_hash(
        &mut self,
        class_id: ClassId,
        executable_class_hash: ExecutableClassHash,
    ) -> ClassHashStorageResult<()> {
        let mut writer = self.writer()?;
        let txn = writer
            .begin_rw_txn()?
            .set_executable_class_hash_v2(&class_id, executable_class_hash)?;
        txn.commit()?;

        Ok(())
    }
}

type FsClassStorageResult<T> = Result<T, FsClassStorageError>;

#[derive(Clone)]
pub struct FsClassStorage {
    pub persistent_root: PathBuf,
    pub class_hash_storage: ClassHashStorage,
}

#[derive(Debug, Error)]
pub enum FsClassStorageError {
    #[error(transparent)]
    ClassHashStorage(#[from] ClassHashStorageError),
    #[error("Class of hash {class_id} not found.")]
    ClassNotFound { class_id: ClassId },
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    RawClass(#[from] RawClassError),
}

impl FsClassStorage {
    pub fn new(config: FsClassStorageConfig) -> FsClassStorageResult<Self> {
        let class_hash_storage = ClassHashStorage::new(config.class_hash_storage_config)?;
        Ok(Self { persistent_root: config.persistent_root, class_hash_storage })
    }

    fn contains_class(&self, class_id: ClassId) -> FsClassStorageResult<bool> {
        Ok(self.get_executable_class_hash(class_id)?.is_some())
    }

    // TODO(Elin): make this more robust; checking file existence is not enough, since by reading
    // it can be deleted.
    fn contains_deprecated_class(&self, class_id: ClassId) -> bool {
        self.get_deprecated_executable_path(class_id).exists()
    }

    /// Returns the directory that will hold classes related to the given class ID.
    /// For a class ID: 0xa1b2c3d4... (rest of hash), the structure is:
    /// a1/
    /// └── b2/
    ///     └── a1b2c3d4.../
    fn get_class_dir(&self, class_id: ClassId) -> PathBuf {
        let class_id = hex::encode(class_id.to_bytes_be());
        let (first_msb_byte, second_msb_byte, _rest_of_bytes) =
            (&class_id[..2], &class_id[2..4], &class_id[4..]);

        PathBuf::from(first_msb_byte).join(second_msb_byte).join(class_id)
    }

    fn get_persistent_dir(&self, class_id: ClassId) -> PathBuf {
        self.persistent_root.join(self.get_class_dir(class_id))
    }

    fn get_persistent_dir_with_create(&self, class_id: ClassId) -> FsClassStorageResult<PathBuf> {
        let path = self.get_persistent_dir(class_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(path)
    }

    fn get_sierra_path(&self, class_id: ClassId) -> PathBuf {
        concat_sierra_filename(&self.get_persistent_dir(class_id))
    }

    fn get_executable_path(&self, class_id: ClassId) -> PathBuf {
        concat_executable_filename(&self.get_persistent_dir(class_id))
    }

    fn get_deprecated_executable_path(&self, class_id: ClassId) -> PathBuf {
        concat_deprecated_executable_filename(&self.get_persistent_dir(class_id))
    }

    fn mark_class_id_as_existent(
        &mut self,
        class_id: ClassId,
        executable_class_hash: ExecutableClassHash,
    ) -> FsClassStorageResult<()> {
        Ok(self.class_hash_storage.set_executable_class_hash(class_id, executable_class_hash)?)
    }

    fn write_class(
        &self,
        class_id: ClassId,
        class: RawClass,
        executable_class: RawExecutableClass,
    ) -> FsClassStorageResult<()> {
        let persistent_dir = self.get_persistent_dir_with_create(class_id)?;
        class.write_to_file(concat_sierra_filename(&persistent_dir))?;
        executable_class.write_to_file(concat_executable_filename(&persistent_dir))?;

        Ok(())
    }

    fn write_deprecated_class(
        &self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> FsClassStorageResult<()> {
        let persistent_dir = self.get_persistent_dir_with_create(class_id)?;
        class.write_to_file(concat_deprecated_executable_filename(&persistent_dir))?;

        Ok(())
    }

    // TODO(Elin): restore use of `write_[deprecated_]class_atomically`, but tmpdir
    // should be located inside the PVC to prevent linking errors.
    #[allow(dead_code)]
    fn write_class_atomically(
        &self,
        class_id: ClassId,
        class: RawClass,
        executable_class: RawExecutableClass,
    ) -> FsClassStorageResult<()> {
        // Write classes to a temporary directory.
        let tmp_dir = create_tmp_dir()?;
        let tmp_dir = tmp_dir.path().join(self.get_class_dir(class_id));
        class.write_to_file(concat_sierra_filename(&tmp_dir))?;
        executable_class.write_to_file(concat_executable_filename(&tmp_dir))?;

        // Atomically rename directory to persistent one.
        let persistent_dir = self.get_persistent_dir_with_create(class_id)?;
        std::fs::rename(tmp_dir, persistent_dir)?;

        Ok(())
    }

    #[allow(dead_code)]
    fn write_deprecated_class_atomically(
        &self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> FsClassStorageResult<()> {
        // Write class to a temporary directory.
        let tmp_dir = create_tmp_dir()?;
        let tmp_dir = tmp_dir.path().join(self.get_class_dir(class_id));
        class.write_to_file(concat_deprecated_executable_filename(&tmp_dir))?;

        // Atomically rename directory to persistent one.
        let persistent_dir = self.get_persistent_dir_with_create(class_id)?;
        std::fs::rename(tmp_dir, persistent_dir)?;

        Ok(())
    }
}

impl ClassStorage for FsClassStorage {
    type Error = FsClassStorageError;

    #[instrument(skip(self, class, executable_class), level = "debug", ret, err)]
    fn set_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
        executable_class_hash: ExecutableClassHash,
        executable_class: RawExecutableClass,
    ) -> Result<(), Self::Error> {
        if self.contains_class(class_id)? {
            return Ok(());
        }

        self.write_class(class_id, class, executable_class)?;
        self.mark_class_id_as_existent(class_id, executable_class_hash)?;

        Ok(())
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_sierra(&self, class_id: ClassId) -> Result<Option<RawClass>, Self::Error> {
        if !self.contains_class(class_id)? {
            return Ok(None);
        }

        let path = self.get_sierra_path(class_id);
        let class =
            RawClass::from_file(path)?.ok_or(FsClassStorageError::ClassNotFound { class_id })?;

        Ok(Some(class))
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_executable(&self, class_id: ClassId) -> Result<Option<RawExecutableClass>, Self::Error> {
        let path = if self.contains_class(class_id)? {
            self.get_executable_path(class_id)
        } else if self.contains_deprecated_class(class_id) {
            self.get_deprecated_executable_path(class_id)
        } else {
            // Class does not exist in storage.
            return Ok(None);
        };

        let class = RawExecutableClass::from_file(path)?
            .ok_or(FsClassStorageError::ClassNotFound { class_id })?;
        Ok(Some(class))
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_executable_class_hash(
        &self,
        class_id: ClassId,
    ) -> Result<Option<ExecutableClassHash>, Self::Error> {
        Ok(self.class_hash_storage.get_executable_class_hash(class_id)?)
    }

    #[instrument(skip(self, class), level = "debug", ret, err)]
    fn set_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> Result<(), Self::Error> {
        if self.contains_deprecated_class(class_id) {
            return Ok(());
        }

        self.write_deprecated_class(class_id, class)?;

        Ok(())
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_deprecated_class(
        &self,
        class_id: ClassId,
    ) -> Result<Option<RawExecutableClass>, Self::Error> {
        if !self.contains_deprecated_class(class_id) {
            return Ok(None);
        }

        let path = self.get_deprecated_executable_path(class_id);
        let class = RawExecutableClass::from_file(path)?
            .ok_or(FsClassStorageError::ClassNotFound { class_id })?;

        Ok(Some(class))
    }
}

impl PartialEq for FsClassStorageError {
    fn eq(&self, other: &Self) -> bool {
        // Only compare enum variants; no need to compare the error values.
        mem::discriminant(self) == mem::discriminant(other)
    }
}

// Utils.

fn concat_sierra_filename(path: &Path) -> PathBuf {
    path.join("sierra")
}

fn concat_executable_filename(path: &Path) -> PathBuf {
    path.join("casm")
}

fn concat_deprecated_executable_filename(path: &Path) -> PathBuf {
    path.join("deprecated_casm")
}

// Creates a tmp directory and returns a owned representation of it.
// As long as the returned directory object is lived, the directory is not deleted.
pub(crate) fn create_tmp_dir() -> FsClassStorageResult<tempfile::TempDir> {
    Ok(tempfile::tempdir()?)
}
