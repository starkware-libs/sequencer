// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![warn(missing_docs)]
//! Configuration utilities for a Starknet node.
//!
//! # Example
//!
//! ```
//! use std::io::Write;
//!
//! use apollo_config::loading::load_and_process_config;
//! use clap::Command;
//! use serde::{Deserialize, Serialize};
//! use tempfile::NamedTempFile;
//!
//! #[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
//! struct ConfigExample {
//!     key: usize,
//! }
//!
//! let config = ConfigExample { key: 42 };
//! let mut tmp = NamedTempFile::new().unwrap();
//! write!(tmp, "{}", serde_json::to_string(&config).unwrap()).unwrap();
//! let path = tmp.path().to_str().unwrap().to_owned();
//!
//! let loaded_config = load_and_process_config::<ConfigExample>(
//!     Command::new("Program"),
//!     vec!["Program".to_owned(), "--config_file".to_owned(), path],
//! )
//! .unwrap();
//! assert_eq!(loaded_config.key, 42);
//! ```

use const_format::formatcp;
use validator::ValidationError;
use validators::ParsedValidationErrors;

/// Arg name for providing a configuration file.
pub const CONFIG_FILE_ARG_NAME: &str = "config_file";
/// Short arg name for providing a configuration file.
pub const CONFIG_FILE_SHORT_ARG_NAME: char = 'f';
/// The config file arg name prepended with a double dash.
pub const CONFIG_FILE_ARG: &str = formatcp!("--{}", CONFIG_FILE_ARG_NAME);

/// Behavior mode configuration for the node.
pub mod behavior_mode;
#[cfg(test)]
mod config_test;
pub mod converters;
pub mod loading;
pub mod presentation;
pub mod secrets;
pub mod validators;

/// Errors at the configuration loading process.
#[allow(missing_docs)]
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error(transparent)]
    CommandInput(#[from] clap::error::Error),
    #[error("{component_config_mismatch}")]
    ComponentConfigMismatch { component_config_mismatch: String },
    #[error(transparent)]
    ConfigValidationError(#[from] ParsedValidationErrors),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    MissingParam(#[from] serde_json::Error),
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
}
