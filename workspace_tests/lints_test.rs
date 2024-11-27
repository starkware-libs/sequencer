use std::collections::HashMap;

use crate::toml_utils::{CrateCargoToml, LintValue, ROOT_TOML};

#[test]
fn test_lints_section_exists() {
    let crates_without_lints: Vec<_> = ROOT_TOML
        .member_cargo_tomls()
        .into_iter()
        .filter(|(_, CrateCargoToml { lints, .. })| lints.is_none())
        .map(|(crate_name, _)| crate_name)
        .collect();
    assert!(
        crates_without_lints.is_empty(),
        "The following crates are missing a [lints] section: {crates_without_lints:#?}."
    );
}

#[test]
fn test_lints_from_workspace() {
    let expected_lints_entry =
        HashMap::<String, LintValue>::from([("workspace".into(), LintValue::Bool(true))]);
    let crates_without_workspace_lints: Vec<_> = ROOT_TOML
        .member_cargo_tomls()
        .into_iter()
        .filter(|(_, CrateCargoToml { lints, .. })| match lints {
            None => false,
            Some(lints) => lints != &expected_lints_entry,
        })
        .map(|(crate_name, _)| crate_name)
        .collect();
    assert!(
        crates_without_workspace_lints.is_empty(),
        "The following crates don't use `workspace = true` in the [lints] section: \
         {crates_without_workspace_lints:?}."
    );
}
