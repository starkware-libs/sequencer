use std::collections::BTreeMap;

use apollo_compilation_utils::class_utils::sierra_program_as_felts_to_big_uint_as_hex;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use cairo_lang_starknet_classes::compiler_version::VersionId as CairoLangVersionId;
use cairo_lang_starknet_classes::contract_class::version_id_from_serialized_sierra_program;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use thiserror::Error;

#[derive(Debug, Error)]
#[cfg_attr(test, derive(PartialEq))]
pub enum VersionIdError {
    #[error("{message}")]
    InvalidVersion { message: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VersionId(pub CairoLangVersionId);

impl std::fmt::Display for VersionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl VersionId {
    pub const MIN: Self = Self(CairoLangVersionId { major: 0, minor: 0, patch: 0 });
    pub const MAX: Self =
        Self(CairoLangVersionId { major: usize::MAX, minor: usize::MAX, patch: usize::MAX });

    pub fn new(major: usize, minor: usize, patch: usize) -> Self {
        Self(CairoLangVersionId { major, minor, patch })
    }

    pub fn from_sierra_program(sierra_program: &[Felt]) -> Result<Self, VersionIdError> {
        let sierra_program_for_compiler = sierra_program_as_felts_to_big_uint_as_hex(
            sierra_program.get(..6).ok_or(VersionIdError::InvalidVersion {
                message: format!(
                    "Failed to retrieve version from the program: insufficient length. Expected \
                     at least 6 felts (got {}).",
                    sierra_program.len()
                ),
            })?,
        );

        let (version_id, _compiler_version_id) = version_id_from_serialized_sierra_program(
            &sierra_program_for_compiler,
        )
        .map_err(|err| VersionIdError::InvalidVersion {
            message: format!("Error extracting version ID from Sierra program: {err}"),
        })?;

        Ok(VersionId(version_id))
    }
}

impl PartialOrd for VersionId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // An implementation of partial_cmp for VersionId.
        fn partial_cmp(
            lhs: &CairoLangVersionId,
            rhs: &CairoLangVersionId,
        ) -> Option<std::cmp::Ordering> {
            if lhs.major != rhs.major {
                return Some(lhs.major.cmp(&rhs.major));
            }
            if lhs.minor != rhs.minor {
                return Some(lhs.minor.cmp(&rhs.minor));
            }
            lhs.patch.partial_cmp(&rhs.patch)
        }

        partial_cmp(&self.0, &other.0)
    }
}

impl SerializeConfig for VersionId {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "major",
                &self.0.major,
                "The major version of the configuration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "minor",
                &self.0.minor,
                "The minor version of the configuration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "patch",
                &self.0.patch,
                "The patch version of the configuration.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
