use validator::Validate;

use super::ConsensusManagerConfig;

// Regression guard for the `#[validate(nested)]` on `consensus_manager_config`: without it,
// validator_derive does not recurse into `ConsensusConfig`, so the nested
// `far_behind_proposal_threshold` range check (and every other nested rule under it) is silently
// skipped on config load. This test exercises the full nested chain, and also confirms the default
// config still validates once nested validation is enabled.
#[test]
fn nested_consensus_config_validation_is_enforced() {
    let mut config = ConsensusManagerConfig::default();
    assert!(config.validate().is_ok(), "default config should be valid");

    config.consensus_manager_config.dynamic_config.far_behind_proposal_threshold = 0;
    assert!(
        config.validate().is_err(),
        "an out-of-range far_behind_proposal_threshold must fail validation via the nested chain"
    );
}
