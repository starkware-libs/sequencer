use std::path::PathBuf;

use rstest::rstest;

use super::AllowedLibfuncsList;

#[rstest]
#[case(AllowedLibfuncsList::Audited, "audited")]
#[case(AllowedLibfuncsList::All, "all")]
#[case(AllowedLibfuncsList::Bundled, "bundled")]
#[case(AllowedLibfuncsList::File(PathBuf::from("/etc/libfuncs.json")), "/etc/libfuncs.json")]
fn allowed_libfuncs_list_round_trips_through_a_json_string(
    #[case] list: AllowedLibfuncsList,
    #[case] expected_serialization: &str,
) {
    let serialized = serde_json::to_value(&list).unwrap();
    assert_eq!(serialized, serde_json::Value::String(expected_serialization.to_string()));
    assert_eq!(serde_json::from_value::<AllowedLibfuncsList>(serialized).unwrap(), list);
}

#[test]
fn empty_allowed_libfuncs_list_is_rejected() {
    assert!(serde_json::from_str::<AllowedLibfuncsList>("\"\"").is_err());
}
