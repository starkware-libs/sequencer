use std::collections::BTreeMap;
use std::fmt;

use cairo_lang_starknet_classes::compiler_version::VersionId as CairoLangVersionId;
use cairo_lang_starknet_classes::contract_class::version_id_from_serialized_sierra_program;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use starknet_sierra_compile::utils::sierra_program_as_felts_to_big_uint_as_hex;
use starknet_types_core::felt::Felt;
use thiserror::Error;

#[derive(Debug, Error)]
#[cfg_attr(test, derive(PartialEq))]
pub enum VersionIdError {
    // TODO(Arni): Consider removing the error message from VersionIdError::InvalidVersion.
    // Error messages should be handled or cause a painc. Talk to product.
    #[error("{message}")]
    InvalidVersion { message: String },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VersionId(pub CairoLangVersionId);

impl std::fmt::Display for VersionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<'de> Deserialize<'de> for VersionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct VersionIdVisitor;

        impl<'de> Visitor<'de> for VersionIdVisitor {
            type Value = VersionId;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("struct VersionId")
            }

            fn visit_map<V>(self, mut map: V) -> Result<VersionId, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut major = None;
                let mut minor = None;
                let mut patch = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        "major" => {
                            if major.is_some() {
                                return Err(serde::de::Error::duplicate_field("major"));
                            }
                            major = Some(map.next_value()?);
                        }
                        "minor" => {
                            if minor.is_some() {
                                return Err(serde::de::Error::duplicate_field("minor"));
                            }
                            minor = Some(map.next_value()?);
                        }
                        "patch" => {
                            if patch.is_some() {
                                return Err(serde::de::Error::duplicate_field("patch"));
                            }
                            patch = Some(map.next_value()?);
                        }
                        _ => {
                            return Err(serde::de::Error::unknown_field(key, FIELDS));
                        }
                    }
                }

                let major = major.ok_or_else(|| serde::de::Error::missing_field("major"))?;
                let minor = minor.ok_or_else(|| serde::de::Error::missing_field("minor"))?;
                let patch = patch.ok_or_else(|| serde::de::Error::missing_field("patch"))?;

                Ok(VersionId::new(major, minor, patch))
            }
        }

        const FIELDS: &[&str] = &["major", "minor", "patch"];
        deserializer.deserialize_struct("VersionId", FIELDS, VersionIdVisitor)
    }
}

impl Serialize for VersionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("VersionId", 3)?;
        s.serialize_field("major", &self.0.major)?;
        s.serialize_field("minor", &self.0.minor)?;
        s.serialize_field("patch", &self.0.patch)?;
        s.end()
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
        if sierra_program_length < 6 {
            return Err(VersionIdError::InvalidVersion {
                message: format!(
                    "Sierra program is too short. got program of length {} which is not long \
                     enough to Sierra program's headers.",
                    sierra_program_length
                ),
            });
        }
        let sierra_program_for_compiler =
            sierra_program_as_felts_to_big_uint_as_hex(&sierra_program[..6]);

        // TODO(Arni): Handle unwrap. map error to VersionIdError.
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
