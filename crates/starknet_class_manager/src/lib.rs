pub mod class_manager;
pub mod class_storage;
pub mod communication;
pub mod config;

use crate::class_manager::ClassManager as GenericClassManager;
use crate::class_storage::FsClassStorage;

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
