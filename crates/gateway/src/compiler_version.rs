use std::collections::BTreeMap;

use cairo_lang_starknet_classes::compiler_version::VersionId as CairoLangVersionId;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::hash::StarkFelt;
use thiserror::Error;
use validator::Validate;

#[derive(Debug, Error)]
#[cfg_attr(test, derive(PartialEq))]
pub enum VersionIdError {
    #[error("{message}")]
    InvalidVersion { message: String },
}

// TODO(Arni): Share this struct with the Cairo lang crate.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct VersionId {
    pub major: usize,
    pub minor: usize,
    pub patch: usize,
}

impl VersionId {
    pub const MIN: Self = Self { major: 0, minor: 0, patch: 0 };
    pub const MAX: Self = Self { major: usize::MAX, minor: usize::MAX, patch: 0 };
}

impl From<&VersionId> for CairoLangVersionId {
    fn from(version: &VersionId) -> Self {
        CairoLangVersionId { major: version.major, minor: version.minor, patch: version.patch }
    }
}

impl std::fmt::Display for VersionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        CairoLangVersionId::from(self).fmt(f)
    }
}

impl VersionId {
    pub fn from_sierra_program(sierra_program: &[StarkFelt]) -> Result<Self, VersionIdError> {
        let sierra_program_length = sierra_program.len();

        if sierra_program_length < 3 {
            return Err(VersionIdError::InvalidVersion {
                message: format!(
                    "Sierra program is too short. got program of length {} which is not long \
                     enough to hold the version field.",
                    sierra_program_length
                ),
            });
        }

        fn get_version_component(
            sierra_program: &[StarkFelt],
            index: usize,
        ) -> Result<usize, VersionIdError> {
            let felt = sierra_program[index];
            felt.try_into().map_err(|_| VersionIdError::InvalidVersion {
                message: format!("version contains a value that is out of range: {}", felt),
            })
        }

        Ok(VersionId {
            major: get_version_component(sierra_program, 0)?,
            minor: get_version_component(sierra_program, 1)?,
            patch: get_version_component(sierra_program, 2)?,
        })
    }
}

impl PartialOrd for VersionId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.major != other.major {
            return Some(self.major.cmp(&other.major));
        }
        if self.minor != other.minor {
            return Some(self.minor.cmp(&other.minor));
        }
        self.patch.partial_cmp(&other.patch)
    }
}

impl SerializeConfig for VersionId {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "major",
                &self.major,
                "The major version of the configuration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "minor",
                &self.minor,
                "The minor version of the configuration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "patch",
                &self.patch,
                "The patch version of the configuration.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
