use std::fmt::Display;
use std::str::FromStr;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::compiler_version::VersionId;
use derive_more::Deref;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use crate::core::CompiledClassHash;
use crate::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use crate::StarknetApiError;

#[cfg(test)]
#[path = "contract_class_test.rs"]
mod contract_class_test;

/// One Felt fits into 32 bytes.
pub const FELT_WIDTH: usize = 32;

#[derive(
    Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(deny_unknown_fields)]
pub enum EntryPointType {
    /// A constructor entry point.
    #[serde(rename = "CONSTRUCTOR")]
    Constructor,
    /// An external entry point.
    #[serde(rename = "EXTERNAL")]
    #[default]
    External,
    /// An L1 handler entry point.
    #[serde(rename = "L1_HANDLER")]
    L1Handler,
}

fn u64_to_usize(val: u64) -> usize {
    val.try_into().expect("Failed to convert u64 version tag to usize.")
}

pub type VersionedCasm = (CasmContractClass, SierraVersion);

/// Represents a raw Starknet contract class.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, derive_more::From)]
pub enum ContractClass {
    V0(DeprecatedContractClass),
    V1(VersionedCasm),
}

impl ContractClass {
    pub fn compiled_class_hash(&self) -> CompiledClassHash {
        match self {
            ContractClass::V0(_) => panic!("Cairo 0 doesn't have compiled class hash."),
            ContractClass::V1((casm_contract_class, _sierra_version)) => {
                // TODO(Aviv): use V2.
                casm_contract_class.hash(&HashVersion::V1)
            }
        }
    }
}

#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, PartialOrd)]
pub struct SierraVersion(Version);

impl From<SierraVersion> for VersionId {
    fn from(val: SierraVersion) -> Self {
        VersionId {
            major: u64_to_usize(val.0.major),
            minor: u64_to_usize(val.0.minor),
            patch: u64_to_usize(val.0.patch),
        }
    }
}

impl SierraVersion {
    /// Version of deprecated contract class (Cairo 0).
    pub const DEPRECATED: Self = Self(Version::new(0, 0, 0));

    pub const LATEST: Self = Self(Version::new(1, 7, 0));

    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self(Version::new(major, minor, patch))
    }

    /// Converts a sierra program to a SierraVersion.
    /// The sierra program is a list of felts.
    /// The first 3 felts are the major, minor and patch version.
    /// The rest of the felts are ignored.
    pub fn extract_from_program<F>(sierra_program: &[F]) -> Result<Self, StarknetApiError>
    // TODO(Aviv): Refactor the implementation to remove generic handling once we standardize to a
    // single type of Felt.
    where
        F: TryInto<u64> + Display + Clone,
        <F as TryInto<u64>>::Error: std::fmt::Display,
    {
        if sierra_program.len() < 3 {
            return Err(StarknetApiError::ParseSierraVersionError(
                "Sierra program length must be at least 3 Felts.".to_string(),
            ));
        }

        let version_components: Vec<u64> = sierra_program
            .iter()
            .take(3)
            .enumerate()
            .map(|(index, felt)| {
                felt.clone().try_into().map_err(|err| {
                    StarknetApiError::ParseSierraVersionError(format!(
                        "Failed to parse Sierra program to Sierra version. Index: {index}, Felt: \
                         {felt}, Error: {err}"
                    ))
                })
            })
            .collect::<Result<_, _>>()?;

        Ok(Self::new(version_components[0], version_components[1], version_components[2]))
    }
}

impl Default for SierraVersion {
    fn default() -> Self {
        Self::LATEST
    }
}

impl FromStr for SierraVersion {
    type Err = StarknetApiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            Version::parse(s)
                .map_err(|_| StarknetApiError::ParseSierraVersionError(s.to_string()))?,
        ))
    }
}

impl From<(u64, u64, u64)> for SierraVersion {
    fn from((major, minor, patch): (u64, u64, u64)) -> Self {
        Self::new(major, minor, patch)
    }
}

/// All relevant information about a declared contract class, including the compiled contract class
/// and other parameters derived from the original declare transaction required for billing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
// TODO(Ayelet,10/02/2024): Change to bytes.
pub struct ClassInfo {
    // TODO(Noa): Consider using Arc.
    pub contract_class: ContractClass,
    pub sierra_program_length: usize,
    pub abi_length: usize,
    pub sierra_version: SierraVersion,
}

impl ClassInfo {
    pub fn bytecode_length(&self) -> usize {
        match &self.contract_class {
            ContractClass::V0(contract_class) => contract_class.bytecode_length(),
            ContractClass::V1((contract_class, _sierra_version)) => contract_class.bytecode.len(),
        }
    }

    pub fn contract_class(&self) -> ContractClass {
        self.contract_class.clone()
    }

    pub fn sierra_program_length(&self) -> usize {
        self.sierra_program_length
    }

    pub fn abi_length(&self) -> usize {
        self.abi_length
    }

    pub fn code_size(&self) -> usize {
        (self.bytecode_length() + self.sierra_program_length())
            // We assume each felt is a word.
            * FELT_WIDTH
            + self.abi_length()
    }

    pub fn new(
        contract_class: &ContractClass,
        sierra_program_length: usize,
        abi_length: usize,
        sierra_version: SierraVersion,
    ) -> Result<Self, StarknetApiError> {
        let (contract_class_version, condition) = match contract_class {
            ContractClass::V0(_) => (0, sierra_program_length == 0),
            ContractClass::V1(_) => (1, sierra_program_length > 0),
        };

        if condition {
            Ok(Self {
                contract_class: contract_class.clone(),
                sierra_program_length,
                abi_length,
                sierra_version,
            })
        } else {
            Err(StarknetApiError::ContractClassVersionSierraProgramLengthMismatch {
                contract_class_version,
                sierra_program_length,
            })
        }
    }
}
