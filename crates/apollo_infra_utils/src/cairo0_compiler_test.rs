use crate::cairo0_compiler::{EXPECTED_CAIRO0_VERSION, PIP_REQUIREMENTS_FILE};

#[test]
fn test_cairo0_version_pip_requirements() {
    let requirements_contents = std::fs::read_to_string(&*PIP_REQUIREMENTS_FILE).unwrap();
    let pip_cairo_lang_version = requirements_contents
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
    assert_eq!(pip_cairo_lang_version, format!("{EXPECTED_CAIRO0_VERSION}"));
}
