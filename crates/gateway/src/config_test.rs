use std::fs::File;
use std::path::{Path, PathBuf};

use clap::Command;
use papyrus_config::dumping::SerializeConfig;
use papyrus_config::loading::load_and_process_config;
use serde::Deserialize;
use validator::Validate;

use crate::config::GatewayNetworkConfig;

const TEST_FILES_FOLDER: &str = "./src/json_files_for_testing";
const NETWORK_CONFIG_FILE: &str = "gateway_network_config.json";

fn get_config_file_path(file_name: &str) -> PathBuf {
    Path::new(TEST_FILES_FOLDER).join(file_name)
}

fn get_config_from_file<T: for<'a> Deserialize<'a>>(
    file_path: PathBuf,
) -> Result<T, papyrus_config::ConfigError> {
    let config_file = File::open(file_path).unwrap();
    load_and_process_config(config_file, Command::new(""), vec![])
}

/// Read the valid config file and validate its content.
fn test_valid_network_config_body(fix: bool) {
    let expected_config = GatewayNetworkConfig { ip: "0.0.0.0".parse().unwrap(), port: 8080 };

    let file_path = get_config_file_path(NETWORK_CONFIG_FILE);
    if fix {
        expected_config.dump_to_file(&vec![], file_path.to_str().unwrap()).unwrap();
    }
    let loaded_config = get_config_from_file::<GatewayNetworkConfig>(file_path).unwrap();

    assert!(loaded_config.validate().is_ok());
    assert_eq!(loaded_config, expected_config);
}

#[test]
fn test_valid_config() {
    test_valid_network_config_body(false);
}

#[test]
#[ignore]
/// Fix the config file for test_valid_config. Run with 'cargo test -- --ignored'.
fn fix_test_valid_config() {
    test_valid_network_config_body(true);
}
