use std::env;
use std::path::{Path, PathBuf};

pub const TEST_FILES_FOLDER: &str = "crates/test_utils/test_files";
pub const CONTRACT_CLASS_FILE: &str = "contract_class.json";

/// Returns the absolute path from the project root.
pub fn get_absolute_path(relative_path: &str) -> PathBuf {
    Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..").join(relative_path)
}
