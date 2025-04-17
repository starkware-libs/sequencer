use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;

use crate::path::resolve_project_relative_path;

/// The local python requirements used to determine the cairo0 compiler version.
static PIP_REQUIREMENTS_FILE: LazyLock<PathBuf> =
    LazyLock::new(|| resolve_project_relative_path("scripts/requirements.txt").unwrap());

/// Verifies that the required Cairo0 compiler is available; panics if unavailable.
pub fn verify_cairo0_compiler_deps() {
    // Python compiler. Verify correct version.
    let cairo_lang_version_output =
        Command::new("sh").arg("-c").arg("pip freeze | grep cairo-lang").output().unwrap().stdout;
    let cairo_lang_version_untrimmed = String::from_utf8(cairo_lang_version_output).unwrap();
    let cairo_lang_version = cairo_lang_version_untrimmed.trim();
    let requirements_contents = fs::read_to_string(&*PIP_REQUIREMENTS_FILE).unwrap();
    let expected_cairo_lang_version = requirements_contents
        .lines()
        .find(|line| line.starts_with("cairo-lang"))
        .unwrap_or_else(|| panic!("Could not find cairo-lang in {:?}.", *PIP_REQUIREMENTS_FILE))
        .trim();

    assert!(
        expected_cairo_lang_version.ends_with(cairo_lang_version),
        "cairo-lang version {expected_cairo_lang_version} not found ({}). Please run:\npip3.9 \
         install -r {:?}\nthen rerun the test.",
        if cairo_lang_version.is_empty() {
            String::from("no installed cairo-lang found")
        } else {
            format!("installed version: {cairo_lang_version}")
        },
        *PIP_REQUIREMENTS_FILE
    );
}
