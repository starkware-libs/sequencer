//! Loads a configuration object from a nested JSON file.
//!
//! Pass `--config_file <path>` to provide the config file. The file must be a standard nested
//! JSON object that serde can deserialize into the target config type directly. `Option<T>` fields
//! are `null` (None) or the actual value (Some) — no `#is_none` encoding needed.

use std::fs::File;
use std::path::PathBuf;

use clap::{value_parser, Arg, Command};
use serde::Deserialize;
use tracing::info;

use crate::{ConfigError, CONFIG_FILE_ARG_NAME, CONFIG_FILE_SHORT_ARG_NAME};

/// Deserializes config of type `T` from a nested JSON file.
///
/// Parses `--config_file <path>` from `args`, opens the file, and deserializes it with serde.
/// All type validation and field presence checks are handled by serde.
pub fn load_and_process_config<T: for<'a> Deserialize<'a>>(
    command: Command,
    args: Vec<String>,
) -> Result<T, ConfigError> {
    let arg_matches = command
        .arg(
            Arg::new(CONFIG_FILE_ARG_NAME)
                .long(CONFIG_FILE_ARG_NAME)
                .short(CONFIG_FILE_SHORT_ARG_NAME)
                .required(true)
                .help("Path to the config file")
                .value_parser(value_parser!(PathBuf)),
        )
        .try_get_matches_from(args)?;

    let config_path =
        arg_matches.get_one::<PathBuf>(CONFIG_FILE_ARG_NAME).expect("config_file is required");

    info!("Loading config from {config_path:?}");
    let file = File::open(config_path)?;
    Ok(serde_json::from_reader(file)?)
}
