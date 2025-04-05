use std::fs::read_to_string;
use std::string::String;

use starknet_api::test_utils::path_in_resources;

pub fn read_resource_file(path_in_resource_dir: &str) -> String {
    let path = path_in_resources(path_in_resource_dir);
    read_to_string(path.to_str().unwrap()).unwrap()
}
