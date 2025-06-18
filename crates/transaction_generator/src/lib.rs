pub mod transaction_generator;

pub const TEST_FILES_FOLDER: &str = "crates/transaction_generator/resources";
pub const CONTRACT_CLASS_FILE: &str = "contract_class.json";
pub const COMPILED_CLASS_HASH_OF_CONTRACT_CLASS: &str =
    "0x01e4f1248860f32c336f93f2595099aaa4959be515e40b75472709ef5243ae17";
// TODO(Arni): Move this file to 'apollo_sierra_multicompile' crate.
pub const FAULTY_ACCOUNT_CLASS_FILE: &str = "faulty_account.sierra.json";
