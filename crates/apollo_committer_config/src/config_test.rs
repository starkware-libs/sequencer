use super::{ApolloCommitterConfig, DEFAULT_COMMIT_DURATION_WARN_THRESHOLD};

/// A config preset that predates `commit_duration_warn_threshold_millis` must still deserialize.
#[test]
fn deserializes_when_warn_threshold_absent() {
    let mut value = serde_json::to_value(ApolloCommitterConfig::default()).unwrap();
    assert!(
        value.as_object_mut().unwrap().remove("commit_duration_warn_threshold_millis").is_some(),
        "field should be present before removal"
    );

    let loaded: ApolloCommitterConfig = serde_json::from_value(value).unwrap();
    assert_eq!(
        loaded.commit_duration_warn_threshold_millis,
        DEFAULT_COMMIT_DURATION_WARN_THRESHOLD
    );
}
