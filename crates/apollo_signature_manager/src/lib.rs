pub mod communication;
pub mod metrics;
pub mod signature_manager;

use std::ops::Deref;

use apollo_infra::component_definitions::ComponentStarter;
use async_trait::async_trait;

use crate::signature_manager::{LocalKeyStore, SignatureManager as GenericSignatureManager};

#[derive(Clone, Debug)]
pub struct LocalKeyStoreSignatureManager(pub GenericSignatureManager<LocalKeyStore>);

impl LocalKeyStoreSignatureManager {
    pub fn new() -> Self {
        Self(GenericSignatureManager::new(LocalKeyStore::new_for_testing()))
    }
}

impl Default for LocalKeyStoreSignatureManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for LocalKeyStoreSignatureManager {
    type Target = GenericSignatureManager<LocalKeyStore>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub use LocalKeyStoreSignatureManager as SignatureManager;

// TODO(Elin): understand how key store would look in production and better define the way the
// signature manager is created.
pub fn create_signature_manager() -> SignatureManager {
    SignatureManager::new()
}

#[async_trait]
impl ComponentStarter for SignatureManager {}
