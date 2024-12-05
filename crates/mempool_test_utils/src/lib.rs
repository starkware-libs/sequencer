pub mod starknet_api_test_utils;

// TODO(Tsabary): unify the 'tests' constants.
pub const TEST_FILES_FOLDER_RELATIVE_TO_PACKAGE_DIR: &str = "test_files";
pub const TEST_FILES_FOLDER: &str = "crates/mempool_test_utils/test_files";
pub const CONTRACT_CLASS_FILE: &str = "contract_class.json";
pub const COMPILED_CLASS_HASH_OF_CONTRACT_CLASS: &str =
    "0x01e4f1248860f32c336f93f2595099aaa4959be515e40b75472709ef5243ae17";
pub const FAULTY_ACCOUNT_CLASS_FILE: &str = "faulty_account.sierra.json";
