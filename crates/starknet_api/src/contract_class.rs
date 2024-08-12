use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use cairo_lang_casm::hints::Hint;
use cairo_lang_starknet_classes::NestedIntList;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::program::Program as CairoVmProgram;
use serde::de::Error as DeserializationError;
use serde::{Deserialize, Deserializer};

use crate::core::EntryPointSelector;
use crate::deprecated_contract_class::{
    sn_api_to_cairo_vm_program,
    EntryPoint,
    EntryPointOffset,
    EntryPointType,
    Program as DeprecatedProgram,
};

/// Compiled contract class.
#[derive(Clone, Debug, Eq, PartialEq, derive_more::From)]
pub enum ContractClass {
    V0(ContractClassV0),
    V1(ContractClassV1),
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct ContractClassV0(pub Arc<ContractClassV0Inner>);
impl Deref for ContractClassV0 {
    type Target = ContractClassV0Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct ContractClassV0Inner {
    #[serde(deserialize_with = "deserialize_program")]
    pub program: CairoVmProgram,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

// V1.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractClassV1(pub Arc<ContractClassV1Inner>);
impl Deref for ContractClassV1 {
    type Target = ContractClassV1Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractClassV1Inner {
    pub program: CairoVmProgram,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPointV1>>,
    pub hints: HashMap<String, Hint>,
    bytecode_segment_lengths: NestedIntList,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct EntryPointV1 {
    pub selector: EntryPointSelector,
    pub offset: EntryPointOffset,
    pub builtins: Vec<BuiltinName>,
}

impl EntryPointV1 {
    pub fn pc(&self) -> usize {
        self.offset.0
    }
}

// V0 utilities.

/// Converts the program type from SN API into a Cairo VM-compatible type.
pub fn deserialize_program<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<CairoVmProgram, D::Error> {
    let deprecated_program = DeprecatedProgram::deserialize(deserializer)?;
    sn_api_to_cairo_vm_program(deprecated_program)
        .map_err(|err| DeserializationError::custom(err.to_string()))
}

/// All relevant information about a declared contract class, including the compiled contract class
/// and other parameters derived from the original declare transaction required for billing.
#[derive(Clone, Debug, Eq, PartialEq)]
// TODO(Ayelet,10/02/2024): Change to bytes.
pub struct ClassInfo {
    pub contract_class: ContractClass,
    pub sierra_program_length: usize,
    pub abi_length: usize,
}
