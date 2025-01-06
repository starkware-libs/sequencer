use core::panic;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedClass;
use starknet_api::state::SierraContractClass;

use crate::{
    Class,
    ClassId,
    ClassManagerClient,
    ClassManagerClientResult,
    ClassManagerError,
    ExecutableClass,
    ExecutableClassHash,
};

pub struct MockClassManagerClient {
    sierras: Arc<Mutex<HashMap<ClassHash, SierraContractClass>>>,
    contracts: Arc<Mutex<HashMap<ClassHash, ExecutableClass>>>,
}

impl MockClassManagerClient {
    pub fn new() -> Self {
        Self {
            sierras: Arc::new(Mutex::new(HashMap::new())),
            contracts: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for MockClassManagerClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ClassManagerClient for MockClassManagerClient {
    async fn add_class(
        &self,
        class_id: ClassId,
        class: Class,
    ) -> ClassManagerClientResult<ExecutableClassHash> {
        if self.sierras.lock().unwrap().insert(class_id, class).is_some() {
            panic!("Class already exists");
        }

        Ok(CompiledClassHash(class_id.0))
    }

    async fn get_executable(&self, class_id: ClassId) -> ClassManagerClientResult<ExecutableClass> {
        let class = self
            .contracts
            .lock()
            .unwrap()
            .get(&class_id)
            .cloned()
            .ok_or_else(|| ClassManagerError::ClassNotFound { class_id })?;

        Ok(class)
    }

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<Class> {
        let class = self
            .sierras
            .lock()
            .unwrap()
            .get(&class_id)
            .cloned()
            .ok_or_else(|| ClassManagerError::ClassNotFound { class_id })?;

        Ok(class)
    }

    async fn add_deprecated_class(
        &self,
        class_id: ClassId,
        class: DeprecatedClass,
    ) -> ClassManagerClientResult<()> {
        if self
            .contracts
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
