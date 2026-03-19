//! Regression tests: bootstrap ERC20 Cairo must compile to the committed Sierra/CASM JSON that
//! `bootstrap_contracts` embeds via `include_str!`.

use std::path::PathBuf;

use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;
use blockifier_test_utils::cairo_compile::{
    allowed_libfuncs_json_path,
    cairo1_compile,
    verify_cairo1_package,
    CompilationArtifacts,
    LibfuncArg,
};
use serde_json::Value;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn compile_bootstrap_erc20() -> (Vec<u8>, Vec<u8>) {
    let version = CAIRO1_COMPILER_VERSION.to_string();
    verify_cairo1_package(&version);

    let cairo_path =
        manifest_dir().join("resources/bootstrap_contracts/cairo1/erc20_testing.cairo");
    let libfunc_arg = LibfuncArg::ListFile(allowed_libfuncs_json_path());

    match cairo1_compile(cairo_path.to_string_lossy().into_owned(), version, libfunc_arg) {
        CompilationArtifacts::Cairo1 { sierra, casm } => (sierra, casm),
        CompilationArtifacts::Cairo0 { .. } => {
            panic!("expected Cairo 1 artifacts for bootstrap erc20_testing.cairo")
        }
    }
}

fn read_expected_json(relative_to_manifest: &str) -> Value {
    let path = manifest_dir().join(relative_to_manifest);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("invalid JSON in {}: {e}", path.display()))
}

#[test]
fn bootstrap_erc20_cairo_matches_committed_sierra_and_casm() {
    let (sierra_bytes, casm_bytes) = compile_bootstrap_erc20();

    let got_sierra: Value =
        serde_json::from_slice(&sierra_bytes).expect("compiled output is valid Sierra JSON");
    let expected_sierra =
        read_expected_json("resources/bootstrap_contracts/cairo1/sierra/erc20_testing.sierra.json");
    assert_eq!(
        got_sierra, expected_sierra,
        "erc20_testing.cairo no longer matches sierra/erc20_testing.sierra.json; recompile with \
         Cairo {} and update the committed artifact",
        CAIRO1_COMPILER_VERSION
    );

    let got_casm: Value =
        serde_json::from_slice(&casm_bytes).expect("compiled output is valid CASM JSON");
    let expected_casm =
        read_expected_json("resources/bootstrap_contracts/cairo1/compiled/erc20_testing.casm.json");
    assert_eq!(
        got_casm, expected_casm,
        "erc20_testing Sierra no longer matches compiled/erc20_testing.casm.json; recompile with \
         Cairo {} and update the committed artifact",
        CAIRO1_COMPILER_VERSION
    );
}
