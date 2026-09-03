// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![warn(missing_docs)]
//! Configuration utilities for a Starknet node.
//!
//! # Example
//!
//! ```
//! use std::fs::write;
//!
//! use apollo_config::loading::load_and_process_config;
//! use apollo_config::CONFIG_FILE_ARG;
//! use clap::Command;
//! use serde::{Deserialize, Serialize};
//! use serde_json::json;
//! use tempfile::TempDir;
//!
//! #[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
//! struct ConfigExample {
//!     key: usize,
//! }
//!
//! // The native loader takes a nested base config and a flat dotted-key secret-overrides file.
//! let dir = TempDir::new().unwrap();
//! let base_path = dir.path().join("config.json");
//! write(&base_path, json!({ "key": 42 }).to_string()).unwrap();
//! let secret_path = dir.path().join("secrets.json");
//! write(&secret_path, json!({ "key": 770 }).to_string()).unwrap();
//!
//! let loaded_config = load_and_process_config::<ConfigExample>(
//!     Command::new("Program"),
//!     vec![
//!         "Program".to_owned(),
//!         CONFIG_FILE_ARG.to_owned(),
//!         base_path.to_str().unwrap().to_owned(),
//!         CONFIG_FILE_ARG.to_owned(),
//!         secret_path.to_str().unwrap().to_owned(),
//!     ],
//!     false,
//! )
//! .unwrap();
//! assert_eq!(loaded_config.key, 770);
//! ```

use clap::parser::MatchesError;
use const_format::formatcp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::Display;
use validator::ValidationError;
use validators::ParsedValidationErrors;

/// Arg name for providing a configuration file.
pub const CONFIG_FILE_ARG_NAME: &str = "config_file";
/// Short arg name for providing a configuration file.
pub const CONFIG_FILE_SHORT_ARG_NAME: char = 'f';
/// The config file arg name prepended with a double dash.
pub const CONFIG_FILE_ARG: &str = formatcp!("--{}", CONFIG_FILE_ARG_NAME);

/// A config indicator for a sub config.
pub const FIELD_SEPARATOR: &str = ".";

/// A nested path of a configuration parameter.
pub type ParamPath = String;

/// Behavior mode configuration for the node.
pub mod behavior_mode;
mod command;
#[cfg(test)]
mod config_test;
pub mod converters;
pub mod loading;
pub mod presentation;
pub mod secrets;
pub mod validators;

/// A serialized type of a configuration parameter.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Display)]
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
    #[error(
        "Native config format requires exactly two config files: the base config and the secret \
         config."
    )]
    NativeModeRequiresTwoConfigFiles,
    #[error("{pointing_param} is not found.")]
    PointerSourceNotFound { pointing_param: String },
    #[error("{target_param} is not found.")]
    PointerTargetNotFound { target_param: String },
    #[error("Received an unexpected parameter: {param_path}.")]
    UnexpectedParam { param_path: String },
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
}
