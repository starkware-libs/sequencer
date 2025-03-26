#[cfg(test)]
#[path = "version_config_test.rs"]
mod version_config_test;

pub const VERSION_PATTERN: &str = "[Vv][0-9]+_[0-9]+(_[0-9]+)?";

#[derive(Eq, PartialEq, Hash)]
/// Labels the jsonRPC versions we have such that there can be multiple versions that are supported,
/// and there can be multiple versions that are deprecated.
/// Supported -> method exposed via the http path "/version_id" (e.g. http://host:port/V0_3_0)
/// Deprecated -> method not exposed.
#[derive(Clone, Copy, Debug)]
pub enum VersionState {
    Supported,
    Deprecated,
}

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub struct VersionId {
    // TODO(yair): change to enum so that the match in get_methods_from_supported_apis can be
    // exhaustive.
    pub name: &'static str,
    pub patch: u8,
}

impl std::fmt::Display for VersionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}_{}", self.name, self.patch)
    }
}

/// latest version must be set as supported
pub const VERSION_CONFIG: &[(VersionId, VersionState)] = &[(VERSION_0_8, VersionState::Supported)];
pub const VERSION_0_8: VersionId = VersionId { name: "V0_8", patch: 0 };
