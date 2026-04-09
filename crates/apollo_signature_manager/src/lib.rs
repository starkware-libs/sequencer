// TODO(Aviv): Delete this mod and use utils from type-rs
mod blake_utils;
pub mod communication;
pub mod metrics;
pub mod signature_manager;

pub use crate::signature_manager::{LocalKeyStore, SignatureManager};

pub fn create_signature_manager() -> SignatureManager {
    SignatureManager::new(LocalKeyStore::new_for_testing())
}
