// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![warn(missing_docs)]
//! Configuration utilities for a Starknet node.
//!
//! # Example
//!
//! ```
//! use std::collections::{BTreeMap, HashSet};
//! use std::fs::File;
//! use std::path::Path;
//!
//! use apollo_config::dumping::{ser_param, SerializeConfig};
//! use apollo_config::loading::load_and_process_config;
//! use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
//! use clap::Command;
//! use serde::{Deserialize, Serialize};
//! use tempfile::TempDir;
//!
//! #[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
//! struct ConfigExample {
//!     key: usize,
//! }
//!
//! impl SerializeConfig for ConfigExample {
//!     fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
//!         BTreeMap::from([ser_param(
//!             "key",
//!             &self.key,
//!             "This is key description.",
//!             ParamPrivacyInput::Public,
//!         )])
//!     }
//! }
//!
//! let dir = TempDir::new().unwrap();
//! let file_path = dir.path().join("config.json");
//! ConfigExample { key: 42 }.dump_to_file(&vec![], &HashSet::new(), file_path.to_str().unwrap());
//! let file = File::open(file_path).unwrap();
//! let loaded_config = load_and_process_config::<ConfigExample>(
//!     file,
//!     Command::new("Program"),
//!     vec!["Program".to_owned(), "--key".to_owned(), "770".to_owned()],
//!     false,
//! )
//! .unwrap();
//! assert_eq!(loaded_config.key, 770);
//! ```

use clap::parser::MatchesError;
use const_format::formatcp;
use dumping::REQUIRED_PARAM_DESCRIPTION_PREFIX;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use validator::ValidationError;
use validators::ParsedValidationErrors;

/// Arg name for providing a configuration file.
pub const CONFIG_FILE_ARG_NAME: &str = "config_file";
/// The config file arg name prepended with a double dash.
pub const CONFIG_FILE_ARG: &str = formatcp!("--{}", CONFIG_FILE_ARG_NAME);

/// A config indicator for optional parameters.
pub const IS_NONE_MARK: &str = "#is_none";
/// A config indicator for a sub config.
pub const FIELD_SEPARATOR: &str = ".";

/// A nested path of a configuration parameter.
pub type ParamPath = String;
/// A description of a configuration parameter.
pub type Description = String;

#[cfg(test)]
mod config_test;

mod command;
pub mod converters;
pub mod dumping;
pub mod loading;
pub mod presentation;
pub mod validators;

/// The privacy level of a config parameter, that received as input from the configs.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum ParamPrivacyInput {
    /// The field is visible only by a secret.
    Private,
    /// The field is visible only to node's users.
    Public,
}

/// The privacy level of a config parameter.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
enum ParamPrivacy {
    /// The field is visible only by a secret.
    Private,
    /// The field is visible only to node's users.
    Public,
    /// The field is not a part of the final config.
    TemporaryValue,
}

impl From<ParamPrivacyInput> for ParamPrivacy {
    fn from(user_param_privacy: ParamPrivacyInput) -> Self {
        match user_param_privacy {
            ParamPrivacyInput::Private => ParamPrivacy::Private,
            ParamPrivacyInput::Public => ParamPrivacy::Public,
        }
    }
}

/// A serialized content of a configuration parameter.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SerializedContent {
    /// Serialized JSON default value.
    #[serde(rename = "value")]
    DefaultValue(Value),
    /// The target from which to take the JSON value of a configuration parameter.
    PointerTarget(ParamPath),
    /// Type of a configuration parameter.
    ParamType(SerializationType),
}

impl SerializedContent {
    fn get_serialization_type(&self) -> Option<SerializationType> {
        match self {
            SerializedContent::DefaultValue(value) => match value {
                // JSON "Number" is handled as PosInt(u64), NegInt(i64), or Float(f64).
                Value::Number(num) => {
                    if num.is_f64() {
                        Some(SerializationType::Float)
                    } else if num.is_u64() {
                        Some(SerializationType::PositiveInteger)
                    } else {
                        Some(SerializationType::NegativeInteger)
                    }
                }
                Value::Bool(_) => Some(SerializationType::Boolean),
                Value::String(_) => Some(SerializationType::String),
                _ => None,
            },
            SerializedContent::PointerTarget(_) => None,
            SerializedContent::ParamType(ser_type) => Some(*ser_type),
        }
    }
}

/// A description and serialized content of a configuration parameter.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SerializedParam {
    /// The description of the parameter.
    pub description: Description,
    /// The content of the parameter.
    #[serde(flatten)]
    pub content: SerializedContent,
    pub(crate) privacy: ParamPrivacy,
}

impl SerializedParam {
    /// Whether the parameter is required.
    // TODO(yair): Find a better way to identify required params - maybe add to the dump.
    pub fn is_required(&self) -> bool {
        self.description.starts_with(REQUIRED_PARAM_DESCRIPTION_PREFIX)
    }

    /// Whether the parameter is private.
    pub fn is_private(&self) -> bool {
        self.privacy == ParamPrivacy::Private
    }
}

/// A serialized type of a configuration parameter.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, strum_macros::Display)]
#[allow(missing_docs)]
pub enum SerializationType {
    Boolean,
    Float,
    NegativeInteger,
    PositiveInteger,
    String,
}

/// Errors at the configuration dumping and loading process.
#[allow(missing_docs)]
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Changing {param_path} from required type {required} to {given} is not allowed.")]
    ChangeRequiredParamType { param_path: String, required: SerializationType, given: Value },
    #[error(transparent)]
    CommandInput(#[from] clap::error::Error),
    #[error(transparent)]
    CommandMatches(#[from] MatchesError),
    #[error("{component_config_mismatch}")]
    ComponentConfigMismatch { component_config_mismatch: String },
    #[error(transparent)]
    ConfigValidationError(#[from] ParsedValidationErrors),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    MissingParam(#[from] serde_json::Error),
    #[error("{pointing_param} is not found.")]
    PointerSourceNotFound { pointing_param: String },
    #[error("{target_param} is not found.")]
    PointerTargetNotFound { target_param: String },
    #[error("Received an unexpected parameter: {param_path}.")]
    UnexpectedParam { param_path: String },
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
}
