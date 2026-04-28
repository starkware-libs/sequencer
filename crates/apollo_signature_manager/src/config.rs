use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Specifies where the node's signing key is stored.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub enum KeySourceConfig {
    /// Use a locally stored key in memory.
    #[default]
    Local,
    /// Fetch the key from Google Secret Manager on first use.
    GoogleSecretManager {
        /// Full GSM resource name, e.g.:
        /// "projects/my-project/secrets/validator-key/versions/latest"
        secret_name: String,
    },
}

/// Configuration for the SignatureManager component.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Validate)]
pub struct SignatureManagerConfig {
    pub key_source: KeySourceConfig,
}

impl SerializeConfig for SignatureManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "key_source",
            &self.key_source,
            "Key source configuration for the signature manager.",
            ParamPrivacyInput::Public,
        )])
    }
}
