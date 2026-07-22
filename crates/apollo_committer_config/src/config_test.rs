use super::{default_block_commit_duration_warn_threshold, ApolloCommitterConfig};

/// A deployed node loads config with schema defaults ignored, so a preset that predates
/// `block_commit_duration_warn_threshold_millis` must still deserialize (via the serde default)
/// rather than fail with a missing-field error.
#[test]
fn deserializes_when_warn_threshold_absent() {
    let mut value = serde_json::to_value(ApolloCommitterConfig::default()).unwrap();
    assert!(
        value
            .as_object_mut()
            .unwrap()
            .remove("block_commit_duration_warn_threshold_millis")
            .is_some(),
        "field should be present before removal"
    );

    let loaded: ApolloCommitterConfig = serde_json::from_value(value).unwrap();
    assert_eq!(
        loaded.block_commit_duration_warn_threshold_millis,
        default_block_commit_duration_warn_threshold()
    );
}
