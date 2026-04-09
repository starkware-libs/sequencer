// TODO(Aviv): Delete this mod and use utils from type-rs
mod blake_utils;
pub mod communication;
pub mod metrics;
pub mod signature_manager;

use apollo_signature_manager_types::SignatureManagerConfig;

pub use crate::signature_manager::SignatureManager;

pub fn create_signature_manager(config: SignatureManagerConfig) -> SignatureManager {
    SignatureManager::new(config).expect("Failed to create SignatureManager")
}
