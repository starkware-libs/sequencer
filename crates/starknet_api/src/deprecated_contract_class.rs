use std::collections::HashMap;
use std::num::ParseIntError;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractEntryPoint;
use itertools::Itertools;
use serde::de::Error as DeserializationError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::core::EntryPointSelector;
use crate::hash::StarkHash;
use crate::serde_utils::deserialize_optional_contract_class_abi_entry_vector;
use crate::StarknetApiError;

/// A deprecated contract class.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    // Starknet does not verify the abi. If we can't parse it, we set it to None.
    #[serde(default, deserialize_with = "deserialize_optional_contract_class_abi_entry_vector")]
    pub abi: Option<Vec<ContractClassAbiEntry>>,
    pub program: Program,
    /// The selector of each entry point is a unique identifier in the program.
    // TODO: Consider changing to IndexMap, since this is used for computing the
    // class hash.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

/// A [ContractClass](`crate::deprecated_contract_class::ContractClass`) abi entry.
// Using untagged so the serialization will be sorted by the keys (the default behavior of Serde for
// untagged enums). We care about the order of the fields in the serialization because it affects
// the class hash calculation.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, untagged)]
pub enum ContractClassAbiEntry {
    Constructor(FunctionAbiEntry<ConstructorType>),
    Event(EventAbiEntry),
    Function(FunctionAbiEntry<FunctionType>),
    L1Handler(FunctionAbiEntry<L1HandlerType>),
    Struct(StructAbiEntry),
}

/// An event abi entry.
// The members of the struct are sorted lexicographically for correct hash computation.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct EventAbiEntry {
    pub data: Vec<TypedParameter>,
    pub keys: Vec<TypedParameter>,
    pub name: String,
    pub r#type: EventType,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    #[default]
    Event,
}

/// A function abi entry.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct FunctionAbiEntry<TYPE> {
    pub inputs: Vec<TypedParameter>,
    pub name: String,
    pub outputs: Vec<TypedParameter>,
    #[serde(rename = "stateMutability", default, skip_serializing_if = "Option::is_none")]
    pub state_mutability: Option<FunctionStateMutability>,
    pub r#type: TYPE,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionType {
    #[default]
    Function,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstructorType {
    #[default]
    Constructor,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum L1HandlerType {
    #[default]
    L1Handler,
}

/// A function state mutability.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum FunctionStateMutability {
    #[serde(rename = "view")]
    #[default]
    View,
}

/// A struct abi entry.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct StructAbiEntry {
    pub members: Vec<StructMember>,
    pub name: String,
    pub size: usize,
    pub r#type: StructType,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StructType {
    #[default]
    Struct,
}

/// A struct member for [StructAbiEntry](`crate::deprecated_contract_class::StructAbiEntry`).
// The members of the struct are sorted lexicographically for correct hash computation.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct StructMember {
    pub name: String,
    pub offset: usize,
    pub r#type: String,
}

/// A program corresponding to a [ContractClass](`crate::deprecated_contract_class::ContractClass`).
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Program {
    #[serde(default)]
    pub attributes: serde_json::Value,
    pub builtins: serde_json::Value,
    #[serde(default)]
    pub compiler_version: serde_json::Value,
    pub data: serde_json::Value,
    #[serde(default)]
    pub debug_info: serde_json::Value,
    #[serde(serialize_with = "serialize_hints_sorted")]
    pub hints: serde_json::Value,
    pub identifiers: serde_json::Value,
    pub main_scope: serde_json::Value,
    pub prime: serde_json::Value,
    pub reference_manager: serde_json::Value,
}

// Serialize hints as a sorted mapping for correct hash computation.
fn serialize_hints_sorted<S>(hints: &serde_json::Value, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if hints.is_null() {
        return serializer.serialize_none();
    }
    let hints_map =
        hints.as_object().ok_or(serde::ser::Error::custom("Hints are not a mapping."))?;
    serializer.collect_map(
        hints_map
            .iter()
            // Parse the keys as integers and sort them.
            .map(|(k, v)| Ok((k.parse::<u32>()?, v)))
            .collect::<Result<Vec<_>, ParseIntError>>()
            .map_err(serde::ser::Error::custom)?
            .iter()
            .sorted_by_key(|(k, _v)| *k)
            // Convert the keys back to strings.
            .map(|(k, v)| (k.to_string(), v)),
    )
}

/// An entry point type of a [ContractClass](`crate::deprecated_contract_class::ContractClass`).
#[derive(
    Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(deny_unknown_fields)]
pub enum EntryPointType {
    /// A constructor entry point.
    #[serde(rename = "CONSTRUCTOR")]
    Constructor,
    /// An external4 entry point.
    #[serde(rename = "EXTERNAL")]
    #[default]
    External,
    /// An L1 handler entry point.
    #[serde(rename = "L1_HANDLER")]
    L1Handler,
}

/// An entry point of a [ContractClass](`crate::deprecated_contract_class::ContractClass`).
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EntryPoint {
    pub selector: EntryPointSelector,
    pub offset: EntryPointOffset,
}

impl TryFrom<CasmContractEntryPoint> for EntryPoint {
    type Error = StarknetApiError;

    fn try_from(value: CasmContractEntryPoint) -> Result<Self, Self::Error> {
        Ok(EntryPoint {
            selector: EntryPointSelector(StarkHash::from(value.selector)),
            offset: EntryPointOffset(value.offset),
        })
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct TypedParameter {
    pub name: String,
    pub r#type: String,
}

/// The offset of an [EntryPoint](`crate::deprecated_contract_class::EntryPoint`).
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EntryPointOffset(
    #[serde(deserialize_with = "number_or_string", serialize_with = "usize_to_hex")] pub usize,
);
impl TryFrom<String> for EntryPointOffset {
    type Error = StarknetApiError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self(hex_string_try_into_usize(&value)?))
    }
}

pub fn number_or_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<usize, D::Error> {
    let usize_value = match Value::deserialize(deserializer)? {
        Value::Number(number) => {
            number.as_u64().ok_or(DeserializationError::custom("Cannot cast number to usize."))?
                as usize
        }
        Value::String(s) => hex_string_try_into_usize(&s).map_err(DeserializationError::custom)?,
        _ => return Err(DeserializationError::custom("Cannot cast value into usize.")),
    };
    Ok(usize_value)
}

fn hex_string_try_into_usize(hex_string: &str) -> Result<usize, std::num::ParseIntError> {
    usize::from_str_radix(hex_string.trim_start_matches("0x"), 16)
}

fn usize_to_hex<S>(value: &usize, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(format!("{:#x}", value).as_str())
}
