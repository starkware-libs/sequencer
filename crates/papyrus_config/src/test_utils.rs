use std::env;
use std::fs::File;

use colored::Colorize;
use starknet_api::test_utils::json_utils::assert_json_eq;
use starknet_infra_utils::path::resolve_project_relative_path;

use crate::dumping::{ConfigPointers, Pointers, SerializeConfig};

/// Loads a config from file and asserts the result json equals to the default config.
pub fn assert_default_config_file_is_up_to_date<T: Default + SerializeConfig>(
    config_binary_name: &str,
    default_config_path: &str,
    config_pointers: &ConfigPointers,
    config_non_pointers_whitelist: &Pointers,
) {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    let from_default_config_file: serde_json::Value =
        serde_json::from_reader(File::open(default_config_path).unwrap()).unwrap();

    // Create a temporary file and dump the default config to it.
    let mut tmp_file_path = env::temp_dir();
    tmp_file_path.push("cfg.json");
    T::default()
        .dump_to_file(
            config_pointers,
            config_non_pointers_whitelist,
            tmp_file_path.to_str().unwrap(),
        )
        .unwrap();

    // Read the dumped config from the file.
    let from_code: serde_json::Value =
        serde_json::from_reader(File::open(tmp_file_path).unwrap()).unwrap();

    let error_message = format!(
        "{}\nDiffs shown below (default config file <<>> dump of {}::default()).",
        format!(
            "Default config file doesn't match the default {} implementation. Please update it \
             using the {} binary.",
            std::any::type_name::<T>(),
            config_binary_name
        )
        .purple()
        .bold(),
        std::any::type_name::<T>()
    );
    assert_json_eq(&from_default_config_file, &from_code, error_message);
}
