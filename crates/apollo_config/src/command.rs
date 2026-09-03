use std::path::PathBuf;

use clap::{value_parser, Arg, ArgMatches, Command};

use crate::{ConfigError, CONFIG_FILE_ARG_NAME, CONFIG_FILE_SHORT_ARG_NAME};

pub(crate) fn get_command_matches(
    command: Command,
    command_input: Vec<String>,
) -> Result<ArgMatches, ConfigError> {
    // The config file flag is the only argument; the config files themselves carry every value.
    let config_file_arg = Arg::new(CONFIG_FILE_ARG_NAME)
        .long(CONFIG_FILE_ARG_NAME)
        .short(CONFIG_FILE_SHORT_ARG_NAME)
        .value_delimiter(',')
        .help("Sets the config files to use")
        .value_parser(value_parser!(PathBuf))
        .num_args(1..) // Allow multiple values
        .action(clap::ArgAction::Append); // Collect multiple occurrences
    Ok(command.args([config_file_arg]).try_get_matches_from(command_input)?)
}
