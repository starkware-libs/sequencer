use std::fs;

use blockifier_test_utils::cairo_compile::{
    generate_allowed_libfuncs_legacy_json,
    CompilationArtifacts,
};
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::{
    FeatureContract,
    CAIRO1_FEATURE_CONTRACTS_DIR,
    SIERRA_CONTRACTS_SUBDIR,
};
use expect_test::expect_file;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::core::CompiledClassHash;
use tracing::info;
use tracing_test::traced_test;

const CAIRO0_FEATURE_CONTRACTS_DIR: &str = "resources/feature_contracts/cairo0";
const COMPILED_CONTRACTS_SUBDIR: &str = "compiled";
const CAIRO0_FIX_COMMAND: &str =
    "env UPDATE_EXPECT=1 FIX_FEATURE_TEST=1 cargo test -p blockifier_test_utils --test \
     feature_contracts_compatibility_test -- --include-ignored verify_feature_contracts_cairo0";

// ======================== Cairo 0 compatibility (committed artifacts) ========================

fn check_cairo0_compilation(
    actual_content: Vec<u8>,
    existing_contents: &str,
    path: &str,
    source_path: String,
) {
    if String::from_utf8(actual_content).unwrap() != existing_contents {
        panic!(
            "{source_path} does not compile to {path}.\nRun `{CAIRO0_FIX_COMMAND}` to fix the \
             existing file according to locally installed `starknet-compile-deprecated`.\n"
        );
    }
}

fn verify_cairo0_contract(contract: &FeatureContract, fix: bool) {
    info!("Compiling {contract:?}...");
    let CompilationArtifacts::Cairo0 { casm } = contract.compile() else {
        unreachable!();
    };
    info!("Done compiling {contract:?}.");
    let existing_compiled_path = contract.get_compiled_path();
    if fix {
        fs::write(&existing_compiled_path, &casm).unwrap();
    }
    let existing_compiled_contents = fs::read_to_string(&existing_compiled_path)
        .unwrap_or_else(|_| panic!("Cannot read {existing_compiled_path}."));
    check_cairo0_compilation(
        casm,
        &existing_compiled_contents,
        &existing_compiled_path,
        contract.get_source_path(),
    );
    assert_eq!(contract.get_compiled_class_hash(&HashVersion::V2), CompiledClassHash::default());
}

// ======================== Enum <-> filesystem consistency ========================

/// For Cairo 0: verifies that each .cairo file has a matching _compiled.json and a matching
/// FeatureContract enum variant. For Cairo 1: verifies that each .cairo source file has a
/// matching FeatureContract enum variant (compiled artifacts are generated on demand).
#[rstest]
fn verify_feature_contracts_match_enum(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
) {
    let directory = match cairo_version {
        CairoVersion::Cairo0 => CAIRO0_FEATURE_CONTRACTS_DIR,
        CairoVersion::Cairo1(RunnableCairo1::Casm) => CAIRO1_FEATURE_CONTRACTS_DIR,
        #[cfg(feature = "cairo_native")]
        CairoVersion::Cairo1(RunnableCairo1::Native) => {
            panic!("This test does not support native CairoVersion.")
        }
    };

    let mut source_basenames_on_filesystem: Vec<String> = vec![];
    for file in fs::read_dir(directory).unwrap() {
        let path = file.unwrap().path();
        if !path.is_file() {
            if let Some(dir_name) = path.file_name() {
                assert!(
                    dir_name == COMPILED_CONTRACTS_SUBDIR || dir_name == SIERRA_CONTRACTS_SUBDIR,
                    "Found directory '{}' in `{directory}`, which should contain only the \
                     `{COMPILED_CONTRACTS_SUBDIR}` or `{SIERRA_CONTRACTS_SUBDIR}` directory.",
                    dir_name.to_string_lossy()
                );
                continue;
            }
        }
        let path_str = path.to_string_lossy();
        assert_eq!(
            path.extension().unwrap(),
            "cairo",
            "Found a non-Cairo file '{path_str}' in `{directory}`"
        );
        source_basenames_on_filesystem
            .push(path.file_stem().unwrap().to_string_lossy().to_string());
    }

    let mut source_basenames_from_enum: Vec<String> = FeatureContract::all_feature_contracts()
        .filter(|contract| contract.cairo_version() == cairo_version)
        .map(|contract| contract.get_non_erc20_base_name().to_string())
        .collect();

    source_basenames_from_enum.sort();
    source_basenames_on_filesystem.sort();
    assert_eq!(source_basenames_from_enum, source_basenames_on_filesystem);

    // For Cairo 0, also verify that committed compiled files exist for each source.
    if cairo_version == CairoVersion::Cairo0 {
        for basename in &source_basenames_on_filesystem {
            let compiled_path =
                format!("{directory}/{COMPILED_CONTRACTS_SUBDIR}/{basename}_compiled.json");
            assert!(
                std::path::Path::new(&compiled_path).exists(),
                "Missing compiled artifact: {compiled_path}"
            );
        }
    }
}

// ======================== Test entrypoints ========================

#[ignore]
#[traced_test]
#[tokio::test(flavor = "multi_thread")]
async fn verify_feature_contracts_cairo0() {
    let fix = std::env::var("FIX_FEATURE_TEST").is_ok();
    let contract_filter = std::env::var("CONTRACT_FILTER").ok();
    let matches_filter = |contract: &FeatureContract| -> bool {
        contract_filter.as_deref().is_none_or(|filter| format!("{contract:?}").contains(filter))
    };
    // TODO(Dori, 1/10/2024): Parallelize Cairo0 recompilation.
    for contract in FeatureContract::all_feature_contracts()
        .filter(|c| c.cairo_version() == CairoVersion::Cairo0)
        .filter(|c| matches_filter(c))
    {
        verify_cairo0_contract(&contract, fix);
    }
}

/// Verifies that `allowed_libfuncs_legacy.json` is in sync with `allowed_libfuncs.json`.
/// Run with `UPDATE_EXPECT=1` to regenerate.
#[test]
fn verify_allowed_libfuncs_legacy() {
    let generated = generate_allowed_libfuncs_legacy_json();
    expect_file!["../resources/allowed_libfuncs_legacy.json"].assert_eq(&generated);
}
