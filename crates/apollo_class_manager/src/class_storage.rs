use std::error::Error;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use apollo_class_manager_config::config::{CachedClassStorageConfig, FsClassStorageConfig};
use apollo_class_manager_types::{CachedClassStorageError, ClassId, ExecutableClassHash};
use apollo_compile_to_casm_types::{RawClass, RawClassError, RawExecutableClass};
use apollo_storage::class_hash::{ClassHashStorageReader, ClassHashStorageWriter};
use apollo_storage::metrics::CLASS_MANAGER_STORAGE_OPEN_READ_TRANSACTIONS;
use apollo_storage::storage_reader_server::ServerConfig;
use apollo_storage::storage_reader_types::GenericStorageReaderServer;
use apollo_storage::StorageConfig;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::ContractClass;
use starknet_api::class_cache::GlobalContractCache;
use thiserror::Error;
use tokio::task::AbortHandle;
use tracing::{debug, instrument, warn};

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

    fn get_executable_class_hash_v2(
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

impl<S> ClassStorage for CachedClassStorage<S>
where
    S: ClassStorage,
    CachedClassStorageError<S::Error>: From<S::Error>,
{
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

        // If compiled_class_hash_v2 exists, it'll be Cairo 1.
        if self.get_executable_class_hash_v2(class_id)?.is_some() {
            let Some(class) = self.storage.get_executable(class_id)? else {
                return Ok(None);
            };
            self.executable_classes.set(class_id, class.clone());
            return Ok(Some(class));
        }

        let Some(class) = self.storage.get_deprecated_class(class_id)? else {
            return Ok(None);
        };
        self.deprecated_classes.set(class_id, class.clone());
        Ok(Some(class))
    }

    #[instrument(skip(self), level = "debug", ret, err)]
    fn get_executable_class_hash_v2(
        &self,
        class_id: ClassId,
    ) -> Result<Option<ExecutableClassHash>, Self::Error> {
        if let Some(compiled_class_hash_v2) = self.executable_class_hashes_v2.get(&class_id) {
            return Ok(Some(compiled_class_hash_v2));
        }

        let Some(compiled_class_hash_v2) = self.storage.get_executable_class_hash_v2(class_id)?
        else {
            return Ok(None);
        };

        self.executable_class_hashes_v2.set(class_id, compiled_class_hash_v2);
        Ok(Some(compiled_class_hash_v2))
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
    // Kept alive to maintain the server running.
    #[allow(dead_code)]
    storage_reader_server_handle: Option<AbortHandle>,
}

impl ClassHashStorage {
    pub fn new(
        storage_config: StorageConfig,
        storage_reader_server_config: ServerConfig,
    ) -> ClassHashStorageResult<Self> {
        let (reader, writer, storage_reader_server) =
            apollo_storage::open_storage_with_metric_and_server(
                storage_config,
                &CLASS_MANAGER_STORAGE_OPEN_READ_TRANSACTIONS,
                storage_reader_server_config,
            )?;

        let storage_reader_server_handle =
            GenericStorageReaderServer::spawn_if_enabled(storage_reader_server);

        Ok(Self { reader, writer: Arc::new(Mutex::new(writer)), storage_reader_server_handle })
    }

    fn writer(&self) -> ClassHashStorageResult<LockedWriter<'_>> {
        Ok(self.writer.lock().expect("Writer is poisoned."))
    }

    #[instrument(skip(self), level = "debug", ret, err)]
    fn get_executable_class_hash_v2(
        &self,
        class_id: ClassId,
    ) -> ClassHashStorageResult<Option<ExecutableClassHash>> {
        Ok(self.reader.begin_ro_txn()?.get_executable_class_hash_v2(&class_id)?)
    }

    #[instrument(skip(self), level = "debug", ret, err)]
    fn set_executable_class_hash_v2(
        &mut self,
        class_id: ClassId,
        executable_class_hash_v2: ExecutableClassHash,
    ) -> ClassHashStorageResult<()> {
        let mut writer = self.writer()?;
        let txn = writer
            .begin_rw_txn()?
            .set_executable_class_hash_v2(&class_id, executable_class_hash_v2)?;
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

impl From<FsClassStorageError> for CachedClassStorageError<FsClassStorageError> {
    fn from(e: FsClassStorageError) -> Self {
        CachedClassStorageError::Storage(e)
    }
}

impl FsClassStorage {
    pub fn new(config: FsClassStorageConfig) -> FsClassStorageResult<Self> {
        let class_hash_storage =
            ClassHashStorage::new(config.class_hash_storage_config, ServerConfig::default())?;
        std::fs::create_dir_all(&config.persistent_root)?;
        Ok(Self { persistent_root: config.persistent_root, class_hash_storage })
    }

    fn contains_class(&self, class_id: ClassId) -> FsClassStorageResult<bool> {
        let marker = self.get_executable_class_hash_v2(class_id)?;
        if let Some(marker) = marker {
            debug!(
                ?class_id,
                executable_class_hash_v2 = %format_args!("{:#064x}", marker.0),
                "Found executable class marker (compiled_class_hash_v2)."
            );
            return Ok(true);
        }

        // This storage relies on the compiled-class-hash-v2 marker as the primary existence signal.
        // If the marker is missing we will treat the class as non-existent, even if the files are
        // present on disk.
        //
        // Log file existence to help debug partial writes / mismatched backends.
        let sierra_path = self.get_sierra_path(class_id);
        let executable_path = self.get_executable_path(class_id);
        let deprecated_path = self.get_deprecated_executable_path(class_id);
        let sierra_exists = sierra_path.exists();
        let executable_exists = executable_path.exists();
        let deprecated_exists = deprecated_path.exists();

        let expected_marker = if executable_exists {
            match RawExecutableClass::from_file(executable_path.clone()) {
                Ok(Some(raw)) => {
                    let value = raw.into_value();
                    match serde_json::from_value::<ContractClass>(value) {
                        Ok(ContractClass::V1((casm, _))) => Some(casm.hash(&HashVersion::V2)),
                        Ok(ContractClass::V0(_)) => None,
                        Err(err) => {
                            debug!(
                                ?class_id,
                                error = %err,
                                "Failed to deserialize CASM file to compute expected marker."
                            );
                            None
                        }
                    }
                }
                Ok(None) => None,
                Err(err) => {
                    debug!(
                        ?class_id,
                        error = %err,
                        "Failed to read CASM file to compute expected marker."
                    );
                    None
                }
            }
        } else {
            None
        };

        if sierra_exists || executable_exists {
            warn!(
                ?class_id,
                persistent_root = %self.persistent_root.display(),
                sierra_path = %sierra_path.display(),
                executable_path = %executable_path.display(),
                sierra_exists,
                executable_exists,
                deprecated_exists,
                expected_executable_class_hash_v2 = expected_marker
                    .as_ref()
                    .map(|h| format!("{:#064x}", h.0)),
                "Executable class marker (compiled_class_hash_v2) is missing but class files exist on disk."
            );
        } else {
            debug!(
                ?class_id,
                persistent_root = %self.persistent_root.display(),
                sierra_exists,
                executable_exists,
                deprecated_exists,
                expected_executable_class_hash_v2 = expected_marker
                    .as_ref()
                    .map(|h| format!("{:#064x}", h.0)),
                "Executable class marker (compiled_class_hash_v2) is missing."
            );
        }

        Ok(false)
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

    fn create_tmp_dir(
        &self,
        class_id: ClassId,
    ) -> FsClassStorageResult<(tempfile::TempDir, PathBuf)> {
        // Compute the final persistent directory for this `class_id`
        let persistent_dir = self.get_persistent_dir(class_id);
        let parent_dir = persistent_dir
            .parent()
            .expect("Class persistent dir should have a parent")
            .to_path_buf();
        std::fs::create_dir_all(&parent_dir)?;
        // Create a temporary directory under the parent of the final persistent directory to ensure
        // `rename` will be atomic.
        let tmp_root = tempfile::tempdir_in(&parent_dir)?;
        // Get the leaf directory name of the final persistent directory.
        let leaf = persistent_dir.file_name().expect("Class dir leaf should exist");
        // Create the temporary directory under the temporary root.
        let tmp_dir = tmp_root.path().join(leaf);
        // Returning `TempDir` since without it the handle would drop immediately and the temp
        // directory would be removed before writes/rename.
        Ok((tmp_root, tmp_dir))
    }

    fn mark_class_id_as_existent(
        &mut self,
        class_id: ClassId,
        executable_class_hash_v2: ExecutableClassHash,
    ) -> FsClassStorageResult<()> {
        Ok(self
            .class_hash_storage
            .set_executable_class_hash_v2(class_id, executable_class_hash_v2)?)
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    fn write_deprecated_class(
        &self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> FsClassStorageResult<()> {
        let persistent_dir = self.get_persistent_dir_with_create(class_id)?;
        class.write_to_file(concat_deprecated_executable_filename(&persistent_dir))?;

        Ok(())
    }

    fn write_class_atomically(
        &self,
        class_id: ClassId,
        class: RawClass,
        executable_class: RawExecutableClass,
    ) -> FsClassStorageResult<()> {
        // Write classes to a temporary directory.
        let (_tmp_root, tmp_dir) = self.create_tmp_dir(class_id)?;
        class.write_to_file(concat_sierra_filename(&tmp_dir))?;
        executable_class.write_to_file(concat_executable_filename(&tmp_dir))?;

        // Atomically rename directory to persistent one.
        let persistent_dir = self.get_persistent_dir_with_create(class_id)?;
        std::fs::rename(tmp_dir, persistent_dir)?;

        Ok(())
    }

    fn write_deprecated_class_atomically(
        &self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> FsClassStorageResult<()> {
        // Write class to a temporary directory.
        let (_tmp_root, tmp_dir) = self.create_tmp_dir(class_id)?;
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
        executable_class_hash_v2: ExecutableClassHash,
        executable_class: RawExecutableClass,
    ) -> Result<(), Self::Error> {
        if self.contains_class(class_id)? {
            return Ok(());
        }

        self.write_class_atomically(class_id, class, executable_class)?;
        self.mark_class_id_as_existent(class_id, executable_class_hash_v2)?;

        Ok(())
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_sierra(&self, class_id: ClassId) -> Result<Option<RawClass>, Self::Error> {
        if !self.contains_class(class_id)? {
            let sierra_path = self.get_sierra_path(class_id);
            debug!(
                ?class_id,
                sierra_path = %sierra_path.display(),
                sierra_exists = sierra_path.exists(),
                "FsClassStorage.get_sierra returning None (class marker missing)."
            );
            return Ok(None);
        }

        let path = self.get_sierra_path(class_id);
        let class =
            RawClass::from_file(path)?.ok_or(FsClassStorageError::ClassNotFound { class_id })?;

        Ok(Some(class))
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_executable(&self, class_id: ClassId) -> Result<Option<RawExecutableClass>, Self::Error> {
        let contains_class = self.contains_class(class_id)?;
        let deprecated_exists = self.contains_deprecated_class(class_id);
        let path = if contains_class {
            self.get_executable_path(class_id)
        } else if deprecated_exists {
            self.get_deprecated_executable_path(class_id)
        } else {
            // Class does not exist in storage.
            let sierra_path = self.get_sierra_path(class_id);
            let executable_path = self.get_executable_path(class_id);
            let deprecated_path = self.get_deprecated_executable_path(class_id);
            debug!(
                ?class_id,
                contains_class,
                deprecated_exists,
                sierra_path = %sierra_path.display(),
                executable_path = %executable_path.display(),
                deprecated_path = %deprecated_path.display(),
                sierra_exists = sierra_path.exists(),
                executable_exists = executable_path.exists(),
                deprecated_file_exists = deprecated_path.exists(),
                "FsClassStorage.get_executable returning None (no marker / no deprecated file)."
            );
            return Ok(None);
        };

        let class = RawExecutableClass::from_file(path)?
            .ok_or(FsClassStorageError::ClassNotFound { class_id })?;
        Ok(Some(class))
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_executable_class_hash_v2(
        &self,
        class_id: ClassId,
    ) -> Result<Option<ExecutableClassHash>, Self::Error> {
        Ok(self.class_hash_storage.get_executable_class_hash_v2(class_id)?)
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

        self.write_deprecated_class_atomically(class_id, class)?;

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
