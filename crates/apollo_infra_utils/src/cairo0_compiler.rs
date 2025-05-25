use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;

use crate::path::resolve_project_relative_path;

/// The local python requirements used to determine the cairo0 compiler version.
static PIP_REQUIREMENTS_FILE: LazyLock<PathBuf> =
    LazyLock::new(|| resolve_project_relative_path("scripts/requirements.txt").unwrap());

static ENTER_VENV_INSTRUCTIONS: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"
python3 -m venv sequencer_venv
. sequencer_venv/bin/activate
pip install -r {:#?}"#,
        *PIP_REQUIREMENTS_FILE
    )
});

/// Verifies that the required Cairo0 compiler is available; panics if unavailable.
pub fn verify_cairo0_compiler_deps() {
    // Python compiler. Verify correct version.
    let cairo_lang_version_output =
        Command::new("sh").arg("-c").arg("pip freeze | grep cairo-lang").output().unwrap().stdout;
    let cairo_lang_version_untrimmed = String::from_utf8(cairo_lang_version_output).unwrap();
    let cairo_lang_version =
        cairo_lang_version_untrimmed.trim().split("==").nth(1).unwrap_or_else(|| {
            panic!(
                "Unexpected cairo-lang version format '{cairo_lang_version_untrimmed}'.  Are you \
                 in a venv? If not, run:\n{}",
                *ENTER_VENV_INSTRUCTIONS
            )
        });
    let requirements_contents = fs::read_to_string(&*PIP_REQUIREMENTS_FILE).unwrap();
    let expected_cairo_lang_version = requirements_contents
        .lines()
        .find(|line| line.starts_with("cairo-lang"))
        .unwrap_or_else(|| panic!("Could not find cairo-lang in {:?}.", *PIP_REQUIREMENTS_FILE))
        .trim()
        .split("==")
        .nth(1)
        .unwrap_or_else(|| {
            panic!(
                "Malformed cairo-lang dependency (expected 'cairo-lang==X') in {:?}.",
                *PIP_REQUIREMENTS_FILE
            )
        });

    assert_eq!(
        expected_cairo_lang_version, cairo_lang_version,
        "cairo-lang version {expected_cairo_lang_version} not found (installed version: \
         {cairo_lang_version}). Run the following commands (enter a python venv and install \
         dependencies) and retry:\n{}",
        *ENTER_VENV_INSTRUCTIONS
    );
}
