use std::path::PathBuf;

use crate::cairo0_compiler::{
    CairoLangVersion,
    EXPECTED_CAIRO0_STARKNET_COMPILE_VERSION,
    EXPECTED_CAIRO0_VERSION,
    PIP_REQUIREMENTS_FILE,
    STARKNET_DEPRECATED_COMPILE_REQUIREMENTS_FILE,
};

fn get_cairo_lang_version_from_requirements(path_to_requirements: &PathBuf) -> String {
    let requirements_contents = std::fs::read_to_string(path_to_requirements).unwrap();
    requirements_contents
        .lines()
        .find(|line| line.starts_with("cairo-lang"))
        .unwrap_or_else(|| panic!("Could not find cairo-lang in {path_to_requirements:?}."))
        .trim()
        .split("==")
        .nth(1)
        .unwrap_or_else(|| {
            panic!(
                "Malformed cairo-lang dependency (expected 'cairo-lang==X') in \
                 {path_to_requirements:?}."
            )
        })
        .to_string()
}

#[test]
fn test_cairo0_version_pip_requirements() {
    let pip_cairo_lang_version = get_cairo_lang_version_from_requirements(&PIP_REQUIREMENTS_FILE);
    assert_eq!(CairoLangVersion(pip_cairo_lang_version.as_str()), EXPECTED_CAIRO0_VERSION);
}

#[test]
fn test_cairo0_starknet_compile_version_pip_requirements() {
    let pip_cairo_lang_version =
        get_cairo_lang_version_from_requirements(&STARKNET_DEPRECATED_COMPILE_REQUIREMENTS_FILE);
    assert_eq!(
        CairoLangVersion(pip_cairo_lang_version.as_str()),
        EXPECTED_CAIRO0_STARKNET_COMPILE_VERSION
    );
}
