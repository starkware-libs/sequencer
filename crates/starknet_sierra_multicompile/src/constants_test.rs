#![cfg(feature = "cairo_native")]
use toml_test_utils::ROOT_TOML;

use crate::constants::REQUIRED_NATIVE_COMPILE_VERSION;

#[test]
fn native_compile_version_test() {
    let workspace_version = ROOT_TOML.workspace_version();
    assert_eq!(REQUIRED_NATIVE_COMPILE_VERSION, workspace_version);
}
