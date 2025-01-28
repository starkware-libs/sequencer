use starknet_class_manager_types::{ClassId, ClassManagerResult, ExecutableClassHash};
use starknet_sierra_multicompile_types::{RawClass, RawExecutableClass};

#[cfg(test)]
#[path = "class_manager_test.rs"]
pub mod class_manager_test;

pub struct ClassManager;

// TODO(Elin): complete implementation.
impl ClassManager {
    pub async fn add_class(
        &mut self,
        _class_id: ClassId,
        _class: RawClass,
    ) -> ClassManagerResult<ExecutableClassHash> {
        unimplemented!()
    }

    pub fn get_executable(&self, _class_id: ClassId) -> ClassManagerResult<RawExecutableClass> {
        unimplemented!()
    }

    pub fn get_sierra(&self, _class_id: ClassId) -> ClassManagerResult<RawClass> {
        unimplemented!()
    }

    pub fn add_deprecated_class(
        &mut self,
        _class_id: ClassId,
        _class: RawExecutableClass,
    ) -> ClassManagerResult<()> {
        unimplemented!()
    }
}
