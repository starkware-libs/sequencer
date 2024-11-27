use std::str::FromStr;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use derive_more::Deref;
use semver::Version;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;

use crate::core::CompiledClassHash;
use crate::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use crate::StarknetApiError;

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

/// Represents a raw Starknet contract class.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, derive_more::From)]
pub enum ContractClass {
    V0(DeprecatedContractClass),
    V1(CasmContractClass),
}

impl ContractClass {
    pub fn compiled_class_hash(&self) -> CompiledClassHash {
        match self {
            ContractClass::V0(_) => panic!("Cairo 0 doesn't have compiled class hash."),
            ContractClass::V1(casm_contract_class) => {
                CompiledClassHash(casm_contract_class.compiled_class_hash())
            }
        }
    }
}

#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct SierraVersion(Version);

impl SierraVersion {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self(Version::new(major, minor, patch))
    }

    /// Version of deprecated contract class.
    pub fn zero() -> Self {
        Self(Version::new(0, 0, 0))
    }
}

impl Default for SierraVersion {
    fn default() -> Self {
        Self::new(1, 0, 0)
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

impl TryFrom<&Vec<Felt>> for SierraVersion {
    type Error = StarknetApiError;

    /// The Sierra version is determined from the first 3 Felts in the Sierra program.
    fn try_from(sierra_program: &Vec<Felt>) -> Result<Self, Self::Error> {
        if sierra_program.len() < 3 {
            return Err(StarknetApiError::ParseSierraVersionError(
                "Sierra program length must be at least 3 Felts.".to_string(),
            ));
        }

        // Closure to map a Felt error to a StarknetApiError.
        let map_felt_to_api_error = |(felt, index): (Felt, usize)| {
            move |err| {
                StarknetApiError::ParseSierraVersionError(format!(
                    "Failed to parse Sierra program to Sierra version. Sierra program index: {}, \
                     Felt value: {}, Error: {}",
                    index, felt, err
                ))
            }
        };

        let major =
            sierra_program[0].try_into().map_err(map_felt_to_api_error((sierra_program[0], 0)))?;
        let minor =
            sierra_program[1].try_into().map_err(map_felt_to_api_error((sierra_program[1], 1)))?;
        let patch =
            sierra_program[2].try_into().map_err(map_felt_to_api_error((sierra_program[2], 2)))?;

        Ok(Self::new(major, minor, patch))
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
            ContractClass::V1(contract_class) => contract_class.bytecode.len(),
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
