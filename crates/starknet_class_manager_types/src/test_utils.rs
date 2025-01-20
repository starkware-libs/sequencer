use core::panic;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedClass;
use starknet_api::felt;
use starknet_api::state::SierraContractClass;

use crate::{
    Class,
    ClassHashes,
    ClassId,
    ClassManagerClient,
    ClassManagerClientResult,
    ClassManagerError,
    ClassStorageError,
    ExecutableClass,
};

pub struct MemoryClassManagerClient {
    sierras: Arc<Mutex<HashMap<ClassHash, SierraContractClass>>>,
    casms: Arc<Mutex<HashMap<ClassHash, ExecutableClass>>>,
}

impl MemoryClassManagerClient {
    pub fn new() -> Self {
        Self {
            sierras: Arc::new(Mutex::new(HashMap::new())),
            casms: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for MemoryClassManagerClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ClassManagerClient for MemoryClassManagerClient {
    async fn add_class(&self, class: Class) -> ClassManagerClientResult<ClassHashes> {
        let classes = self.sierras.lock().unwrap();
        let class_id = ClassHash(felt!(classes.len()));
        if classes.insert(class_id, class).is_some() {
            panic!("Class already exists");
        }

        Ok((class_id, CompiledClassHash(class_id.0)))
    }

    async fn get_executable(&self, class_id: ClassId) -> ClassManagerClientResult<ExecutableClass> {
        let class = self.casms.lock().unwrap().get(&class_id).cloned().ok_or_else(|| {
            ClassManagerError::ClassStorageError(ClassStorageError::ClassNotFound { class_id })
        })?;

        Ok(class)
    }

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<Class> {
        let class = self.sierras.lock().unwrap().get(&class_id).cloned().ok_or_else(|| {
            ClassManagerError::ClassStorageError(ClassStorageError::ClassNotFound { class_id })
        })?;

        Ok(class)
    }

    async fn add_deprecated_class(
        &self,
        class_id: ClassId,
        class: DeprecatedClass,
    ) -> ClassManagerClientResult<()> {
        if self
            .casms
            .lock()
            .unwrap()
            .insert(class_id, starknet_api::contract_class::ContractClass::V0(class))
            .is_some()
        {
            panic!("Class already exists");
        }

        Ok(())
    }
}
