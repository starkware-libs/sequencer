use std::fs;

use blockifier_test_utils::cairo_compile::{verify_cairo1_package, CompilationArtifacts};
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::{
    FeatureContract,
    CAIRO1_FEATURE_CONTRACTS_DIR,
    SIERRA_CONTRACTS_SUBDIR,
};
use pretty_assertions::assert_eq;
use rstest::rstest;
use tracing::info;
use tracing_test::traced_test;

const CAIRO0_FEATURE_CONTRACTS_DIR: &str = "resources/feature_contracts/cairo0";
const COMPILED_CONTRACTS_SUBDIR: &str = "compiled";
const FIX_COMMAND: &str = "FIX_FEATURE_TEST=1 cargo test -p blockifier_test_utils --test \
                           feature_contracts_compatibility_test -- --include-ignored";

pub enum FeatureContractMetadata {
    Cairo0(Cairo0FeatureContractMetadata),
    Cairo1(Cairo1FeatureContractMetadata),
}

impl FeatureContractMetadata {
    pub fn compiled_path(&self) -> String {
        match self {
            FeatureContractMetadata::Cairo0(data) => data.compiled_path.clone(),
            FeatureContractMetadata::Cairo1(data) => data.compiled_path.clone(),
        }
    }

    pub fn sierra_path(&self) -> String {
        match self {
            FeatureContractMetadata::Cairo0(_) => panic!("No sierra path for Cairo0 contracts."),
            FeatureContractMetadata::Cairo1(data) => data.sierra_path.clone(),
        }
    }
}

pub struct Cairo0FeatureContractMetadata {
    pub source_path: String,
    pub base_filename: String,
    pub compiled_path: String,
}

pub struct Cairo1FeatureContractMetadata {
    pub source_path: String,
    pub base_filename: String,
    pub compiled_path: String,
    pub sierra_path: String,
}

// To fix Cairo0 feature contracts, first enter a python venv and install the requirements (see
// `enter_venv_instructions` for how to do this). Then, run the FIX_COMMAND above.

// To fix Cairo1 feature contracts, first clone the Cairo repo and checkout the required tag.
// The repo should be located next to the sequencer repo:
// <WORKSPACE_DIR>/
// - sequencer/
// - cairo/
// Then, run the FIX_COMMAND above.

// Checks that:
// 1. `TEST_CONTRACTS` dir exists and contains only `.cairo` files and the subdirectory
// `COMPILED_CONTRACTS_SUBDIR`.
// 2. for each `X.cairo` file in `TEST_CONTRACTS` there exists an `X_compiled.json` file in
// `COMPILED_CONTRACTS_SUBDIR` which equals `starknet-compile-deprecated X.cairo --no_debug_info`.
async fn verify_feature_contracts_compatibility(fix: bool, cairo_version: CairoVersion) {
    match cairo_version {
        // TODO(Dori, 1/10/2024): Parallelize Cairo0 recompilation.
        CairoVersion::Cairo0 => {
            for contract in FeatureContract::all_feature_contracts()
                .filter(|contract| contract.cairo_version() == cairo_version)
            {
                verify_feature_contracts_compatibility_logic(&contract, fix);
            }
        }
        CairoVersion::Cairo1(RunnableCairo1::Casm) => {
            // Prepare cairo packages.
            let mut download_task_set = tokio::task::JoinSet::new();
            for version in FeatureContract::all_cairo1_casm_compiler_versions() {
                download_task_set.spawn(async move { verify_cairo1_package(&version).await });
            }
            info!(
                "Verifying Cairo1 packages for versions {:?}.",
                FeatureContract::all_cairo1_casm_compiler_versions()
            );
            download_task_set.join_all().await;
            info!("Cairo1 packages verified.");
            // Verify feature contracts.
            let mut task_set = tokio::task::JoinSet::new();
            for contract in FeatureContract::all_cairo1_casm_feature_contracts() {
                info!("Spawning task for {contract:?}.");
                task_set.spawn(verify_feature_contracts_compatibility_logic_async(contract, fix));
            }
            info!("Done spawning tasks for contract compilation. Awaiting them...");
            task_set.join_all().await;
            info!("Done awaiting tasks for contract compilation.");
        }
        #[cfg(feature = "cairo_native")]
        CairoVersion::Cairo1(RunnableCairo1::Native) => {
            panic!("This test does not support native CairoVersion.")
        }
    }
}

async fn verify_feature_contracts_compatibility_logic_async(contract: FeatureContract, fix: bool) {
    verify_feature_contracts_compatibility_logic(&contract, fix);
}

fn check_compilation(
    actual_content: Vec<u8>,
    existing_contents: &str,
    path: &str,
    source_path: String,
) {
    if String::from_utf8(actual_content).unwrap() != existing_contents {
        panic!(
            "{source_path} does not compile to {path}.\nRun `{FIX_COMMAND}` to fix the existing \
             file according to locally installed `starknet-compile-deprecated`.\n"
        );
    }
}

fn compare_compilation_data(contract: &FeatureContract) {
    let expected_compiled_raw_output = contract.compile();
    let existing_compiled_path = contract.get_compiled_path();
    let existing_compiled_contents = fs::read_to_string(&existing_compiled_path)
        .unwrap_or_else(|_| panic!("Cannot read {existing_compiled_path}."));

    match expected_compiled_raw_output {
        CompilationArtifacts::Cairo0 { casm } => {
            check_compilation(
                casm,
                &existing_compiled_contents,
                &existing_compiled_path,
                contract.get_source_path(),
            );
        }
        CompilationArtifacts::Cairo1 { casm, sierra } => {
            // TODO(Aviv): Remove this if after fixing sierra file of cairo steps contract.
            if !matches!(contract, FeatureContract::CairoStepsTestContract) {
                check_compilation(
                    casm,
                    &existing_compiled_contents,
                    &existing_compiled_path,
                    contract.get_source_path(),
                );

                let sierra_compiled_path = contract.get_sierra_path();
                let existing_sierra_contents = fs::read_to_string(&sierra_compiled_path)
                    .unwrap_or_else(|_| panic!("Cannot read {sierra_compiled_path}."));
                check_compilation(
                    sierra,
                    &existing_sierra_contents,
                    &sierra_compiled_path,
                    contract.get_source_path(),
                );
            }
        }
    }
}

fn verify_feature_contracts_compatibility_logic(contract: &FeatureContract, fix: bool) {
    // Compare output of cairo-file on file with existing compiled file.
    info!("Compiling {contract:?}...");
    let expected_compiled_raw_output = contract.compile();
    info!("Done compiling {contract:?}.");
    let existing_compiled_path = contract.get_compiled_path();
    if fix {
        match expected_compiled_raw_output {
            CompilationArtifacts::Cairo0 { ref casm } => {
                fs::write(&existing_compiled_path, casm).unwrap();
            }
            CompilationArtifacts::Cairo1 { ref casm, ref sierra } => {
                fs::write(&existing_compiled_path, casm).unwrap();
                fs::write(contract.get_sierra_path(), sierra).unwrap();
            }
        }
    }

    compare_compilation_data(contract);
}

/// Verifies that the feature contracts directory contains the expected contents, and returns
/// the feature contracts metadata.
fn verify_and_get_files(cairo_version: CairoVersion) -> Vec<FeatureContractMetadata> {
    let mut paths = vec![];
    let directory = match cairo_version {
        CairoVersion::Cairo0 => CAIRO0_FEATURE_CONTRACTS_DIR,
        CairoVersion::Cairo1(RunnableCairo1::Casm) => CAIRO1_FEATURE_CONTRACTS_DIR,
        #[cfg(feature = "cairo_native")]
        CairoVersion::Cairo1(RunnableCairo1::Native) => {
            panic!("This test does not support native CairoVersion.")
        }
    };
    let compiled_extension = match cairo_version {
        CairoVersion::Cairo0 => "_compiled.json",
        CairoVersion::Cairo1(RunnableCairo1::Casm) => ".casm.json",
        #[cfg(feature = "cairo_native")]
        CairoVersion::Cairo1(RunnableCairo1::Native) => {
            panic!("This test does not support native CairoVersion.")
        }
    };
    for file in fs::read_dir(directory).unwrap() {
        let path = file.unwrap().path();

        // Verify `TEST_CONTRACTS` file and directory structure.
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

        let file_name = path.file_stem().unwrap().to_string_lossy();
        let existing_compiled_path =
            format!("{directory}/{COMPILED_CONTRACTS_SUBDIR}/{file_name}{compiled_extension}");

        match cairo_version {
            CairoVersion::Cairo0 => {
                paths.push(FeatureContractMetadata::Cairo0(Cairo0FeatureContractMetadata {
                    source_path: path_str.to_string(),
                    base_filename: file_name.to_string(),
                    compiled_path: existing_compiled_path,
                }))
            }

            CairoVersion::Cairo1(RunnableCairo1::Casm) => {
                let existing_sierra_path =
                    format!("{directory}/{SIERRA_CONTRACTS_SUBDIR}/{file_name}.sierra.json");
                paths.push(FeatureContractMetadata::Cairo1(Cairo1FeatureContractMetadata {
                    source_path: path_str.to_string(),
                    base_filename: file_name.to_string(),
                    compiled_path: existing_compiled_path,
                    sierra_path: existing_sierra_path,
                }));
            }
            #[cfg(feature = "cairo_native")]
            CairoVersion::Cairo1(RunnableCairo1::Native) => {
                panic!("This test does not support native CairoVersion.")
            }
        }
    }

    paths
}

// Native and Casm have the same contracts, therefore should have the same enum, so we exclude
// Native CairoVersion from this test.
#[rstest]
fn verify_feature_contracts_match_enum(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
) {
    let mut compiled_paths_from_enum: Vec<String> = FeatureContract::all_feature_contracts()
        .filter(|contract| contract.cairo_version() == cairo_version)
        .map(|contract| contract.get_compiled_path())
        .collect();

    let mut compiled_paths_on_filesystem = match cairo_version {
        CairoVersion::Cairo0 => verify_and_get_files(cairo_version)
            .into_iter()
            .map(|metadata| metadata.compiled_path().to_string())
            .collect(),
        CairoVersion::Cairo1(RunnableCairo1::Casm) => {
            let (compiled_paths_on_filesystem, mut sierra_paths_on_filesystem): (
                Vec<String>,
                Vec<String>,
            ) = verify_and_get_files(cairo_version)
                .into_iter()
                .map(|metadata| (metadata.compiled_path(), metadata.sierra_path()))
                .collect();

            let mut sierra_paths_from_enum: Vec<String> = FeatureContract::all_feature_contracts()
                .filter(|contract| contract.cairo_version() == cairo_version)
                .map(|contract| contract.get_sierra_path())
                .collect();

            sierra_paths_from_enum.sort();
            sierra_paths_on_filesystem.sort();
            assert_eq!(sierra_paths_from_enum, sierra_paths_on_filesystem);
            compiled_paths_on_filesystem
        }

        #[cfg(feature = "cairo_native")]
        CairoVersion::Cairo1(RunnableCairo1::Native) => {
            panic!("This test does not support native CairoVersion.")
        }
    };
    compiled_paths_from_enum.sort();
    compiled_paths_on_filesystem.sort();
    assert_eq!(compiled_paths_from_enum, compiled_paths_on_filesystem);
}

async fn verify_feature_contracts_test_body(cairo_version: CairoVersion) {
    let fix_features = std::env::var("FIX_FEATURE_TEST").is_ok();
    verify_feature_contracts_compatibility(fix_features, cairo_version).await;
}

// Native and Casm have the same contracts and compiled files, as we only save the sierra for
// Native, so we exclude Native CairoVersion from these tests.
#[ignore]
#[traced_test]
#[tokio::test(flavor = "multi_thread")]
async fn verify_feature_contracts_cairo0() {
    verify_feature_contracts_test_body(CairoVersion::Cairo0).await;
}

#[ignore]
#[traced_test]
#[tokio::test(flavor = "multi_thread")]
async fn verify_feature_contracts_cairo1() {
    verify_feature_contracts_test_body(CairoVersion::Cairo1(RunnableCairo1::Casm)).await;
}
