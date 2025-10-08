pub mod class_manager;
mod class_storage;
pub mod communication;
pub mod metrics;

// Re-export selected items from the now-private class_storage module.
pub use class_storage::{
    CachedClassStorage,
    ClassHashStorage,
    ClassHashStorageError,
    ClassStorage,
    FsClassStorage,
    FsClassStorageError,
};

use crate::class_manager::ClassManager as GenericClassManager;

pub struct FsClassManager(pub GenericClassManager<FsClassStorage>);

impl Clone for FsClassManager {
    fn clone(&self) -> Self {
        let GenericClassManager { config, compiler, classes } = &self.0;

        FsClassManager(GenericClassManager {
            config: config.clone(),
            compiler: compiler.clone(),
            classes: classes.clone(),
        })
    }
}

pub use FsClassManager as ClassManager;

#[cfg(any(feature = "testing", test))]
pub mod test_utils;
