use starknet_sierra_multicompile::constants::REQUIRED_CAIRO_NATIVE_VERSION;
use toml_test_utils::ROOT_TOML;

#[test]
fn cairo_native_version_test() {
    let workspace_version = ROOT_TOML.workspace_version();
    assert_eq!(REQUIRED_CAIRO_NATIVE_VERSION, workspace_version);
}
