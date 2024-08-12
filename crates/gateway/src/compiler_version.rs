use std::collections::BTreeMap;

use cairo_lang_starknet_classes::compiler_version::VersionId as CairoLangVersionId;
use cairo_lang_starknet_classes::contract_class::version_id_from_serialized_sierra_program;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_sierra_compile::utils::sierra_program_as_felts_to_big_uint_as_hex;
use starknet_types_core::felt::Felt;
use thiserror::Error;
use validator::Validate;

#[derive(Debug, Error)]
#[cfg_attr(test, derive(PartialEq))]
pub enum VersionIdError {
    // TODO(Arni): Consider removing the error message from VersionIdError::InvalidVersion.
    // Error messages should be handled or cause a painc. Talk to product.
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
    pub const MAX: Self = Self { major: usize::MAX, minor: usize::MAX, patch: usize::MAX };
}

impl From<VersionId> for CairoLangVersionId {
    fn from(version: VersionId) -> Self {
        CairoLangVersionId { major: version.major, minor: version.minor, patch: version.patch }
    }
}

impl From<CairoLangVersionId> for VersionId {
    fn from(version: CairoLangVersionId) -> Self {
        VersionId { major: version.major, minor: version.minor, patch: version.patch }
    }
}

impl std::fmt::Display for VersionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        CairoLangVersionId::from(*self).fmt(f)
    }
}

impl VersionId {
    pub fn from_sierra_program(sierra_program: &[Felt]) -> Result<Self, VersionIdError> {
        let sierra_program_length = sierra_program.len();

        if sierra_program_length < 6 {
            return Err(VersionIdError::InvalidVersion {
                message: format!(
                    "Sierra program is too short. Got program of length {}, which is not long \
                     enough for Sierra program's headers.",
                    sierra_program_length
                ),
            });
        }
        let sierra_program_for_compiler =
            sierra_program_as_felts_to_big_uint_as_hex(&sierra_program[..6]);

        let (version_id, _compiler_version_id) = version_id_from_serialized_sierra_program(
            &sierra_program_for_compiler,
        )
        .map_err(|err| VersionIdError::InvalidVersion {
            message: format!("Error extracting version ID from Sierra program: {err}"),
        })?;

        Ok(version_id.into())
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
