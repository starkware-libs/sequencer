use cairo_lang_starknet_classes::compiler_version::{current_sierra_version_id, VersionId};
use semver::Version;

use super::SierraVersion;

#[test]
pub fn test_last_sierra_version() {
    let SierraVersion(Version { major, minor, patch, .. }) = SierraVersion::LATEST;

    // Construct VersionId from SierraVersion::LATEST.
    let latest_sierra_version = VersionId {
        major: major.try_into().unwrap(),
        minor: minor.try_into().unwrap(),
        patch: patch.try_into().unwrap(),
    };

    // Expected version from Cairo repo.
    let expected_version = current_sierra_version_id();

    assert_eq!(
        latest_sierra_version, expected_version,
        "SierraVersion::LATEST does not match the expected version: {expected_version:?}.
        Please update SierraVersion::LATEST."
    );
}
