<<<<<<< Updated upstream
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use starknet_class_manager_types::{
    ClassId,
    ClassManagerResult,
    ClassStorageError,
    ExecutableClassHash,
};
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sierra_compile_types::{RawClass, SharedSierraCompilerClient};

pub mod communication;

struct ClassManagerConfig;

struct ClassManager<S: ClassStorage> {
=======
use starknet_class_manager_types::{ClassId, ClassManagerResult, ExecutableClassHash};
use starknet_sierra_compile_types::{RawClass, RawExecutableClass, SharedSierraCompilerClient};

use crate::class_storage::{CachedClassStorage, CachedClassStorageConfig, ClassStorage};
pub mod class_storage;

#[derive(Clone, Copy, Debug)]
struct ClassManagerConfig {
    cached_class_storage_config: CachedClassStorageConfig,
}

pub struct ClassManager<S: ClassStorage> {
>>>>>>> Stashed changes
    config: ClassManagerConfig,
    compiler: SharedSierraCompilerClient,
    classes: CachedClassStorage<S>,
}

impl<S: ClassStorage> ClassManager<S> {
    fn new(config: ClassManagerConfig, compiler: SharedSierraCompilerClient, storage: S) -> Self {
<<<<<<< Updated upstream
        Self { config, compiler, classes: CachedClassStorage::new(storage) }
=======
        Self {
            config,
            compiler,
            classes: CachedClassStorage::new(config.cached_class_storage_config, storage),
        }
>>>>>>> Stashed changes
    }
}

impl<S: ClassStorage> ClassManager<S> {
    async fn add_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
    ) -> ClassManagerResult<ExecutableClassHash> {
        let (raw_executable_class, executable_class_hash) =
            self.compiler.compile(class.clone()).await?;

        self.classes.set_class(class_id, class, executable_class_hash, raw_executable_class)?;

        Ok(executable_class_hash)
    }

<<<<<<< Updated upstream
    fn get_executable(&self, class_id: ClassId) -> ClassManagerResult<RawClass> {
=======
    fn get_executable(&self, class_id: ClassId) -> ClassManagerResult<RawExecutableClass> {
>>>>>>> Stashed changes
        Ok(self.classes.get_executable(class_id)?)
    }

    fn get_sierra(&self, class_id: ClassId) -> ClassManagerResult<RawClass> {
        Ok(self.classes.get_sierra(class_id)?)
    }

    fn add_deprecated_class(
        &mut self,
        class_id: ClassId,
<<<<<<< Updated upstream
        class: RawClass,
=======
        class: RawExecutableClass,
>>>>>>> Stashed changes
    ) -> ClassManagerResult<()> {
        self.classes.set_deprecated_class(class_id, class)?;
        Ok(())
    }
}
<<<<<<< Updated upstream

impl<S: ClassStorage> ComponentStarter for ClassManager<S> {}

type ClassStorageResult<T> = Result<T, ClassStorageError>;

trait ClassStorage: Send + Sync {
    fn set_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
        executable_class_hash: ExecutableClassHash,
        executable_class: RawClass,
    ) -> ClassStorageResult<()>;

    fn get_sierra(&self, class_id: ClassId) -> ClassStorageResult<RawClass>;

    fn get_executable(&self, class_id: ClassId) -> ClassStorageResult<RawClass>;

    fn set_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
    ) -> ClassStorageResult<()>;
}

struct NoopClassStorage;

impl ClassStorage for NoopClassStorage {
    fn set_class(
        &mut self,
        _class_id: ClassId,
        _class: RawClass,
        _executable_class_hash: ExecutableClassHash,
        _executable_class: RawClass,
    ) -> ClassStorageResult<()> {
        Ok(())
    }

    fn get_sierra(&self, class_id: ClassId) -> ClassStorageResult<RawClass> {
        Err(ClassStorageError::ClassNotFound { class_id })
    }

    fn get_executable(&self, class_id: ClassId) -> ClassStorageResult<RawClass> {
        Err(ClassStorageError::ClassNotFound { class_id })
    }

    fn set_deprecated_class(
        &mut self,
        _class_id: ClassId,
        _class: RawClass,
    ) -> ClassStorageResult<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ClassData {
    class: RawClass,
    executable_class_hash: ExecutableClassHash,
    executable_class: RawClass,
}

struct CachedClassStorage<S: ClassStorage> {
    storage: S,
    classes: Mutex<HashMap<ClassId, ClassData>>,
    deprecated_classes: Mutex<HashMap<ClassId, RawClass>>,
}

impl<S: ClassStorage> CachedClassStorage<S> {
    fn new(storage: S) -> Self {
        Self {
            storage,
            classes: Mutex::new(HashMap::new()),
            deprecated_classes: Mutex::new(HashMap::new()),
        }
    }

    fn classes(&self) -> MutexGuard<'_, HashMap<ClassId, ClassData>> {
        self.classes.lock().expect("Failed to acquire classes lock.")
    }
}

impl<S: ClassStorage> ClassStorage for CachedClassStorage<S> {
    fn set_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
        executable_class_hash: ExecutableClassHash,
        executable_class: RawClass,
    ) -> ClassStorageResult<()> {
        let class_data = ClassData {
            class: class.clone(),
            executable_class_hash,
            executable_class: executable_class.clone(),
        };

        let mut classes = self.classes.lock().expect("Failed to acquire classes lock.");
        self.classes().insert(class_id, class_data);
        self.storage.set_class(class_id, class, executable_class_hash, executable_class)
    }

    fn get_sierra(&self, class_id: ClassId) -> ClassStorageResult<RawClass> {
        let classes = self.classes.lock().expect("Failed to acquire classes lock.");
        if let Some(class_data) = classes.get(&class_id) {
            Ok(class_data.class.clone())
        } else {
            self.storage.get_sierra(class_id)
        }
    }

    fn get_executable(&self, class_id: ClassId) -> ClassStorageResult<RawClass> {
        let classes = self.classes.lock().expect("Failed to acquire classes lock.");
        if let Some(class_data) = classes.get(&class_id) {
            Ok(class_data.executable_class.clone())
        } else {
            self.storage.get_executable(class_id)
        }
    }

    fn set_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
    ) -> ClassStorageResult<()> {
        let mut deprecated_classes =
            self.deprecated_classes.lock().expect("Failed to acquire deprecated classes lock.");
        deprecated_classes.insert(class_id, class.clone());
        self.storage.set_deprecated_class(class_id, class)
    }
}
=======
>>>>>>> Stashed changes
