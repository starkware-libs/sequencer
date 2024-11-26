use std::fs;

use blockifier::test_utils::cairo_compile::{
    prepare_group_tag_compiler_deps,
    CompilationArtifacts,
};
use blockifier::test_utils::contracts::{
    FeatureContract,
    CAIRO1_FEATURE_CONTRACTS_DIR,
    SIERRA_CONTRACTS_SUBDIR,
};
use blockifier::test_utils::CairoVersion;
use pretty_assertions::assert_eq;
use rstest::rstest;

const CAIRO0_FEATURE_CONTRACTS_DIR: &str = "feature_contracts/cairo0";
#[cfg(feature = "cairo_native")]
const NATIVE_FEATURE_CONTRACTS_DIR: &str = "feature_contracts/cairo_native";
const COMPILED_CONTRACTS_SUBDIR: &str = "compiled";
const FIX_COMMAND: &str = "FIX_FEATURE_TEST=1 cargo test -p blockifier --test \
                           feature_contracts_compatibility_test --features testing -- \
                           --include-ignored";

pub enum FeatureContractMetadata {
    Cairo0(DeprecatedFeatureContractMetadata),
    Cairo1(InnerFeatureContractMetadata),
    #[cfg(feature = "cairo_native")]
    Native(InnerFeatureContractMetadata),
}

impl FeatureContractMetadata {
    pub fn compiled_path(&self) -> &str {
        match self {
            FeatureContractMetadata::Cairo0(data) => &data.compiled_path,
            FeatureContractMetadata::Cairo1(data) => &data.compiled_path,
            #[cfg(feature = "cairo_native")]
            FeatureContractMetadata::Native(data) => &data.compiled_path,
        }
    }

    pub fn sierra_path(&self) -> &str {
        match self {
            FeatureContractMetadata::Cairo0(_) => panic!("No sierra path for Cairo0 contracts."),
            FeatureContractMetadata::Cairo1(data) => &data.sierra_path,
            #[cfg(feature = "cairo_native")]
            FeatureContractMetadata::Native(data) => &data.sierra_path,
        }
    }
}
pub struct DeprecatedFeatureContractMetadata {
    pub source_path: String,
    pub base_filename: String,
    pub compiled_path: String,
}
pub struct InnerFeatureContractMetadata {
    pub source_path: String,
    pub base_filename: String,
    pub compiled_path: String,
    pub sierra_path: String,
}

// To fix Cairo0 feature contracts, first enter a python venv and install the requirements:
// ```
// python -m venv tmp_venv
// . tmp_venv/bin/activate
// pip install -r crates/blockifier/tests/requirements.txt
// ```
// Then, run the FIX_COMMAND above.

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
fn verify_feature_contracts_compatibility(fix: bool, cairo_version: CairoVersion) {
    // TODO(Dori, 1/10/2024): Parallelize this test.
    match cairo_version {
        CairoVersion::Cairo0 => {
            for contract in FeatureContract::all_feature_contracts()
                .filter(|contract| contract.cairo_version() == cairo_version)
            {
                verify_feature_contracts_compatibility_logic(&contract, fix);
            }
        }
        _ => {
            for (tag_and_tool_chain, feature_contracts) in
                FeatureContract::cairo1_feature_contracts_by_tag()
            {
                prepare_group_tag_compiler_deps(tag_and_tool_chain.0.clone());
                // TODO(Meshi 01/01/2025) Make this loop concurrent
                for contract in feature_contracts
                    .into_iter()
                    .filter(|contract| contract.cairo_version() == cairo_version)
                {
                    verify_feature_contracts_compatibility_logic(&contract, fix);
                }
            }
        }
    }
}

fn verify_feature_contracts_compatibility_logic(contract: &FeatureContract, fix: bool) {
    // Compare output of cairo-file on file with existing compiled file.
    let expected_compiled_raw_output = contract.compile();
    let expected_compiled_output = expected_compiled_raw_output.get_compiled_output();

    let existing_compiled_path = contract.get_compiled_path();

    if fix {
        fs::write(&existing_compiled_path, &expected_compiled_output).unwrap();
        match expected_compiled_raw_output {
            CompilationArtifacts::Cairo0 { .. } => {}
            #[cfg(feature = "cairo_native")]
            CompilationArtifacts::Cairo1Native { .. } => {}
            CompilationArtifacts::Cairo1 { sierra, .. } => {
                fs::write(contract.get_sierra_path(), &sierra).unwrap();
            }
        }
    }
    let existing_compiled_contents = fs::read_to_string(&existing_compiled_path)
        .unwrap_or_else(|_| panic!("Cannot read {existing_compiled_path}."));

    if String::from_utf8(expected_compiled_output).unwrap() != existing_compiled_contents {
        panic!(
            "{} does not compile to {existing_compiled_path}.\nRun `{FIX_COMMAND}` to fix the \
             existing file according to locally installed `starknet-compile-deprecated`.\n",
            contract.get_source_path()
        );
    }
}

/// Verifies that the feature contracts directory contains the expected contents, and returns
/// the feature contracts metadata.
fn verify_and_get_files(cairo_version: CairoVersion) -> Vec<FeatureContractMetadata> {
    let mut paths = vec![];
    let directory = match cairo_version {
        CairoVersion::Cairo0 => CAIRO0_FEATURE_CONTRACTS_DIR,
        CairoVersion::Cairo1 => CAIRO1_FEATURE_CONTRACTS_DIR,
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => NATIVE_FEATURE_CONTRACTS_DIR,
    };
    let compiled_extension = match cairo_version {
        CairoVersion::Cairo0 => "_compiled.json",
        CairoVersion::Cairo1 => ".casm.json",
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => ".sierra.json",
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
                paths.push(FeatureContractMetadata::Cairo0(DeprecatedFeatureContractMetadata {
                    source_path: path_str.to_string(),
                    base_filename: file_name.to_string(),
                    compiled_path: existing_compiled_path,
                }))
            }
            CairoVersion::Cairo1 => {
                let existing_sierra_path =
                    format!("{directory}/{SIERRA_CONTRACTS_SUBDIR}/{file_name}.sierra.json");
                paths.push(FeatureContractMetadata::Cairo1(InnerFeatureContractMetadata {
                    source_path: path_str.to_string(),
                    base_filename: file_name.to_string(),
                    compiled_path: existing_compiled_path,
                    sierra_path: existing_sierra_path,
                }));
            }
            #[cfg(feature = "cairo_native")]
            CairoVersion::Native => {
                let existing_sierra_path =
                    format!("{directory}/{SIERRA_CONTRACTS_SUBDIR}/{file_name}.sierra.json");
                paths.push(FeatureContractMetadata::Native(InnerFeatureContractMetadata {
                    source_path: path_str.to_string(),
                    base_filename: file_name.to_string(),
                    compiled_path: existing_compiled_path,
                    sierra_path: existing_sierra_path,
                }));
            }
        }
    }

    paths
}

#[rstest]
fn verify_feature_contracts_match_enum(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] cairo_version: CairoVersion,
) {
    let mut compiled_paths_from_enum: Vec<String> = FeatureContract::all_feature_contracts()
        .filter(|contract| contract.cairo_version() == cairo_version)
        .map(|contract| contract.get_compiled_path())
        .collect();
    let mut compiled_paths_on_filesystem: Vec<String>;
    match cairo_version {
        CairoVersion::Cairo0 => {
            compiled_paths_on_filesystem = verify_and_get_files(cairo_version)
                .into_iter()
                .map(|metadata| metadata.compiled_path().to_string())
                .collect();
        }
        CairoVersion::Cairo1 => {
            let mut sierra_paths_on_filesystem: Vec<String>;
            (compiled_paths_on_filesystem, sierra_paths_on_filesystem) =
                verify_and_get_files(cairo_version)
                    .into_iter()
                    .map(|metadata| {
                        (metadata.compiled_path().to_string(), metadata.sierra_path().to_string())
                    })
                    .collect();

            let mut sierra_paths_from_enum: Vec<String> = FeatureContract::all_feature_contracts()
                .filter(|contract| contract.cairo_version() == cairo_version)
                .map(|contract| contract.get_sierra_path())
                .collect();
            sierra_paths_from_enum.sort();
            sierra_paths_on_filesystem.sort();
            assert_eq!(sierra_paths_from_enum, sierra_paths_on_filesystem);
        }
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => {
            let mut sierra_paths_on_filesystem: Vec<String>;
            (compiled_paths_on_filesystem, sierra_paths_on_filesystem) =
                verify_and_get_files(cairo_version)
                    .into_iter()
                    .map(|metadata| {
                        (metadata.compiled_path().to_string(), metadata.sierra_path().to_string())
                    })
                    .collect();
            let mut sierra_paths_from_enum: Vec<String> = FeatureContract::all_feature_contracts()
                .filter(|contract| contract.cairo_version() == cairo_version)
                .map(|contract| contract.get_sierra_path())
                .collect();
            sierra_paths_from_enum.sort();
            sierra_paths_on_filesystem.sort();
            assert_eq!(sierra_paths_from_enum, sierra_paths_on_filesystem);
        }
    }

    compiled_paths_from_enum.sort();
    compiled_paths_on_filesystem.sort();
    assert_eq!(compiled_paths_from_enum, compiled_paths_on_filesystem);
}

// todo(rdr): find the right way to feature verify native contracts as well
#[rstest]
#[ignore]
fn verify_feature_contracts(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] cairo_version: CairoVersion,
) {
    let fix_features = std::env::var("FIX_FEATURE_TEST").is_ok();
    verify_feature_contracts_compatibility(fix_features, cairo_version)
}
