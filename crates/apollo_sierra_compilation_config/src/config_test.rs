use super::AllowedLibfuncsList;

/// The deployment config sets this param by these exact strings.
#[test]
fn allowed_libfuncs_list_serializes_to_its_config_spelling() {
    for (list, expected_serialization) in [
        (AllowedLibfuncsList::Audited, "audited"),
        (AllowedLibfuncsList::All, "all"),
        (AllowedLibfuncsList::Bundled, "bundled"),
    ] {
        let serialized = serde_json::to_value(list).unwrap();
        assert_eq!(serialized, serde_json::Value::String(expected_serialization.to_string()));
        assert_eq!(serde_json::from_value::<AllowedLibfuncsList>(serialized).unwrap(), list);
    }
}
