pub mod starknet_api_test_utils;

pub const TEST_FILES_FOLDER: &str = "crates/mempool_test_utils/resources";
pub const CONTRACT_CLASS_FILE: &str = "contract_class.json";
pub const COMPILED_CLASS_HASH_OF_CONTRACT_CLASS: &str =
    "0x24d8d75cba029fa1896dd4d9424df66f8b279b058b4c6f3245b369c78c5d156";
// TODO(Arni): Move this file to 'apollo_sierra_multicompile' crate.
pub const FAULTY_ACCOUNT_CLASS_FILE: &str = "faulty_account.sierra.json";
