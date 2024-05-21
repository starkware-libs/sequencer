use std::fs::File;
use std::path::Path;

use clap::Command;
use papyrus_config::loading::load_and_process_config;
use validator::Validate;

use crate::config::GatewayConfig;

const TEST_FILES_FOLDER: &str = "./src/json_files_for_testing";
const CONFIG_FILE: &str = "gateway_config.json";

fn get_config_file(file_name: &str) -> Result<GatewayConfig, papyrus_config::ConfigError> {
    let config_file = File::open(Path::new(TEST_FILES_FOLDER).join(file_name)).unwrap();
    load_and_process_config::<GatewayConfig>(config_file, Command::new(""), vec![])
}

#[test]
fn test_valid_config() {
    // Read the valid config file and validate its content.
    let expected_config = GatewayConfig { ip: "0.0.0.0".parse().unwrap(), port: 8080 };

    let loaded_config = get_config_file(CONFIG_FILE).unwrap();

    assert!(loaded_config.validate().is_ok());
    assert_eq!(loaded_config, expected_config);
}
