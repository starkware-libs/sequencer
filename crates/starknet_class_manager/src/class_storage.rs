use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::mem;
use std::path::{Path, PathBuf};

use papyrus_storage::class_hash::{ClassHashStorageReader, ClassHashStorageWriter};
use starknet_api::class_cache::GlobalContractCache;
use starknet_api::core::ChainId;
use starknet_class_manager_types::{ClassId, ClassStorageError, ExecutableClassHash};
use starknet_sierra_multicompile_types::{RawClass, RawExecutableClass};
use thiserror::Error;

#[cfg(test)]
#[path = "class_storage_test.rs"]
mod class_storage_test;

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

#[derive(Debug)]
pub struct ClassHashStorageConfig {
    pub path_prefix: PathBuf,
    pub enforce_file_exists: bool,
    pub max_size: usize,
}

#[derive(Debug, Error)]
pub enum ClassHashStorageError {
    #[error("Class of hash: {class_id} not found")]
    ClassNotFound { class_id: ClassId },
    #[error(transparent)]
    Storage(#[from] papyrus_storage::StorageError),
}

type ClassHashStorageResult<T> = Result<T, ClassHashStorageError>;

pub struct ClassHashStorage {
    reader: papyrus_storage::StorageReader,
    writer: papyrus_storage::StorageWriter,
}

impl ClassHashStorage {
    pub fn new(config: ClassHashStorageConfig) -> ClassHashStorageResult<Self> {
        let storage_config = papyrus_storage::StorageConfig {
            db_config: papyrus_storage::db::DbConfig {
                path_prefix: config.path_prefix,
                chain_id: ChainId::Other("UnusedChainID".to_string()),
                enforce_file_exists: config.enforce_file_exists,
                max_size: config.max_size,
                growth_step: 1 << 20, // 1MB.
                ..Default::default()
            },
            scope: papyrus_storage::StorageScope::StateOnly,
            mmap_file_config: papyrus_storage::mmap_file::MmapFileConfig {
                max_size: 1 << 30,        // 1GB.
                growth_step: 1 << 20,     // 1MB.
                max_object_size: 1 << 10, // 1KB; a class hash is 32B.
            },
        };
        let (reader, writer) = papyrus_storage::open_storage(storage_config)?;

        Ok(Self { reader, writer })
    }

    #[cfg(test)]
    pub fn new_for_testing() -> Self {
        let config = ClassHashStorageConfig {
            path_prefix: tempfile::tempdir().unwrap().path().to_path_buf(),
            enforce_file_exists: false,
            max_size: 1 << 20, // 1MB.
        };

        Self::new(config).unwrap()
    }

    fn get_executable_class_hash(
        &self,
        class_id: ClassId,
    ) -> ClassHashStorageResult<ExecutableClassHash> {
        self.reader
            .begin_ro_txn()?
            .get_executable_class_hash(&class_id)?
            .ok_or(ClassHashStorageError::ClassNotFound { class_id })
    }

    fn set_executable_class_hash(
        &mut self,
        class_id: ClassId,
        executable_class_hash: ExecutableClassHash,
    ) -> ClassHashStorageResult<()> {
        let txn = self
            .writer
            .begin_rw_txn()?
            .set_executable_class_hash(&class_id, executable_class_hash)?;
        txn.commit()?;
        Ok(())
    }
}

type FsClassStorageResult<T> = Result<T, FsClassStorageError>;

pub struct FsClassStorage {
    persistent_root: PathBuf,
    class_hash_storage: ClassHashStorage,
}

#[derive(Debug, Error)]
pub enum FsClassStorageError {
    #[error(transparent)]
    ClassHashStorage(#[from] ClassHashStorageError),
    #[error("Class of hash: {class_id} not found")]
    ClassNotFound { class_id: ClassId },
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    WriteError(#[from] serde_json::Error),
}

impl FsClassStorage {
    pub fn new(persistent_root: PathBuf, class_hash_storage: ClassHashStorage) -> Self {
        Self { persistent_root, class_hash_storage }
    }

    #[cfg(test)]
    pub fn new_for_testing() -> Self {
        let test_persistent_root = tempfile::tempdir().unwrap().path().to_path_buf();
        let class_hash_storage = ClassHashStorage::new_for_testing();

        Self::new(test_persistent_root, class_hash_storage)
    }

    fn contains_class(&self, class_id: ClassId) -> bool {
        self.get_executable_class_hash(class_id).is_ok()
    }

    fn contains_deprecated_class(&self, class_id: ClassId) -> bool {
        self.get_executable_path(class_id).exists()
    }

    /// Returns the directory that will hold classes related to the given class ID.
    /// For a class ID: 0xa1b2c3... (rest of hash), the structure is:
    /// a1/
    /// └── b2c3.../
    fn get_class_dir(&self, class_id: ClassId) -> PathBuf {
        let class_id = hex::encode(class_id.to_bytes_be());
        let (first_msb_byte, rest_of_bytes) = class_id.split_at(2);
        PathBuf::from(first_msb_byte).join(rest_of_bytes)
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

    fn read_file(&self, path: PathBuf) -> FsClassStorageResult<serde_json::Value> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Ok(serde_json::from_reader(reader)?)
    }

    fn write_file(&self, path: PathBuf, data: serde_json::Value) -> FsClassStorageResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open a file for writing, delete content if exists
        // (should not happen, since the file is a unique temporary file).
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .expect("Failing to open file with given options is impossible");

        // Write to file.
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &data)?;

        Ok(())
    }
}

impl ClassStorage for FsClassStorage {
    type Error = FsClassStorageError;

    fn set_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
        executable_class_hash: ExecutableClassHash,
        executable_class: RawExecutableClass,
    ) -> Result<(), Self::Error> {
        if self.contains_class(class_id) {
            return Ok(());
        }

        // Write classes to a temporary directory.
        let tmp_dir = tempfile::tempdir()?;
        let tmp_dir = tmp_dir.path().join(self.get_class_dir(class_id));
        self.write_file(concat_sierra_filename(&tmp_dir), class.0)?;
        self.write_file(concat_executable_filename(&tmp_dir), executable_class.0)?;

        // Atomically rename directory to persistent one.
        let persistent_dir = self.get_persistent_dir_with_create(class_id)?;
        std::fs::rename(tmp_dir, persistent_dir)?;

        // Mark class as existent.
        self.class_hash_storage.set_executable_class_hash(class_id, executable_class_hash)?;

        Ok(())
    }

    fn get_sierra(&self, class_id: ClassId) -> Result<RawClass, Self::Error> {
        if !self.contains_class(class_id) {
            return Err(FsClassStorageError::ClassNotFound { class_id });
        }

        let path = self.get_sierra_path(class_id);
        let raw_class = self.read_file(path)?;
        Ok(RawClass(raw_class))
    }

    fn get_executable(&self, class_id: ClassId) -> Result<RawExecutableClass, Self::Error> {
        if !self.contains_class(class_id) {
            return Err(FsClassStorageError::ClassNotFound { class_id });
        }

        let path = self.get_executable_path(class_id);
        let raw_class = self.read_file(path)?;
        Ok(RawExecutableClass(raw_class))
    }

    fn get_executable_class_hash(
        &self,
        class_id: ClassId,
    ) -> Result<ExecutableClassHash, Self::Error> {
        Ok(self.class_hash_storage.get_executable_class_hash(class_id)?)
    }

    fn set_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> Result<(), Self::Error> {
        if self.contains_deprecated_class(class_id) {
            return Ok(());
        }

        // Write class to a temporary directory.
        let tmp_dir = tempfile::tempdir()?.into_path();
        let tmp_dir = tmp_dir.join(self.get_class_dir(class_id));
        self.write_file(concat_executable_filename(&tmp_dir), class.0)?;

        // Atomically rename directory to persistent one.
        let persistent_dir = self.get_persistent_dir_with_create(class_id)?;
        std::fs::rename(tmp_dir, persistent_dir)?;

        Ok(())
    }
}

impl PartialEq for FsClassStorageError {
    fn eq(&self, other: &Self) -> bool {
        use FsClassStorageError::*;

        match (self, other) {
            (ClassNotFound { class_id }, ClassNotFound { class_id: other_class_id }) => {
                class_id == other_class_id
            }
            // Only compare enum variants; no need to compare the error values.
            _ => mem::discriminant(self) == mem::discriminant(other),
        }
    }
}

// Utils.

fn concat_sierra_filename(path: &Path) -> PathBuf {
    path.join("sierra")
}

fn concat_executable_filename(path: &Path) -> PathBuf {
    path.join("casm")
}
