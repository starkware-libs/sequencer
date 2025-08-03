pub mod starknet_api_test_utils;

pub const TEST_FILES_FOLDER: &str = "crates/mempool_test_utils/resources";
pub const CONTRACT_CLASS_FILE: &str = "contract_class.json";
pub const COMPILED_CLASS_HASH_OF_CONTRACT_CLASS: &str =
    "0x00000000508a1bf69901c60099ff4759ae4438ef239d8f58858df5632e4e5e6f";
// TODO(Arni): Move this file to 'apollo_sierra_multicompile' crate.
pub const FAULTY_ACCOUNT_CLASS_FILE: &str = "faulty_account.sierra.json";
