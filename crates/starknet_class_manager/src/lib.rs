pub mod class_manager;
pub mod class_storage;

use crate::class_manager::ClassManager as GenericClassManager;
use crate::class_storage::FsClassStorage;

pub type FsClassManager = GenericClassManager<FsClassStorage>;
pub use FsClassManager as ClassManager;
