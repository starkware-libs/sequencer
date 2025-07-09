use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::string::FromUtf8Error;

use papyrus_common::python_json::PythonJsonFormatter;
use serde::{Deserialize, Serialize, Serializer};
use sha3::digest::Digest;
use starknet_api::contract_class::EntryPointType;
use starknet_api::deprecated_contract_class::{ContractClass, EntryPointV0};
use starknet_api::state::truncated_keccak;
use starknet_types_core::felt::Felt;

#[derive(Debug, thiserror::Error)]
pub enum HintedClassHashError {
    #[error(transparent)]
    FromUtf8(#[from] FromUtf8Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

/// Our version of the cairo contract definition used to deserialize and re-serialize a modified
/// version for a hash of the contract definition.
///
/// The implementation uses `serde_json::Value` extensively for the unknown/undefined structure, and
/// the correctness of this implementation depends on the following features of serde_json:
///
/// - feature `raw_value` has to be enabled for the thrown away `program.debug_info`
/// - feature `preserve_order` has to be disabled, as we want everything sorted
/// - feature `arbitrary_precision` has to be enabled, as there are big integers in the input
// TODO(Yoav): For more efficiency, have only borrowed types of serde_json::Value.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CairoContractDefinition<'a> {
    /// Contract ABI, which has no schema definition.
    pub abi: serde_json::Value,

    /// Main program definition.
    #[serde(borrow)]
    pub program: CairoProgram<'a>,

    /// The contract entry points.
    ///
    /// These are left out of the re-serialized version with the ordering requirement to a
    /// Keccak256 hash.
    #[serde(skip_serializing)]
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPointV0>>,
}

// It's important that this is ordered alphabetically because the fields need to be in sorted order
// for the keccak hashed representation.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CairoProgram<'a> {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attributes: Vec<serde_json::Value>,

    #[serde(borrow)]
    pub builtins: Vec<Cow<'a, str>>,

    // Added in Starknet 0.10, so we have to handle this not being present.
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub compiler_version: Option<Cow<'a, str>>,

    #[serde(borrow)]
    pub data: Vec<Cow<'a, str>>,

    // Serialize as None for compatibility with Python.
    #[serde(borrow, serialize_with = "serialize_as_none")]
    pub debug_info: Option<&'a serde_json::value::RawValue>,

    // Important that this is ordered by the numeric keys, not lexicographically
    pub hints: BTreeMap<u64, Vec<serde_json::Value>>,

    pub identifiers: serde_json::Value,

    #[serde(borrow)]
    pub main_scope: Cow<'a, str>,

    // Unlike most other integers, this one is hex string. We don't need to interpret it, it just
    // needs to be part of the hashed output.
    #[serde(borrow)]
    pub prime: Cow<'a, str>,

    pub reference_manager: serde_json::Value,
}

fn serialize_as_none<S>(
    _: &Option<&serde_json::value::RawValue>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_none()
}

/// `std::io::Write` adapter for Keccak256; we don't need the serialized version in
/// compute_class_hash, but we need the truncated_keccak hash.
///
/// When debugging mismatching hashes, it might be useful to check the length of each before trying
/// to find the wrongly serialized spot. Example length > 500kB.
#[derive(Default)]
struct KeccakWriter(sha3::Keccak256);

impl std::io::Write for KeccakWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // noop is fine, we'll finalize after the write phase.
        Ok(())
    }
}

pub fn compute_cairo_hinted_class_hash(
    contract_class: &ContractClass,
) -> Result<Felt, HintedClassHashError> {
    let contract_definition_vec = serde_json::to_vec(contract_class)?;
    let contract_definition: CairoContractDefinition<'_> =
        serde_json::from_slice(&contract_definition_vec)?;
    let mut string_buffer = vec![];

    let mut ser = serde_json::Serializer::with_formatter(&mut string_buffer, PythonJsonFormatter);
    contract_definition.serialize(&mut ser)?;

    let raw_json_output = String::from_utf8(string_buffer)?;

    let mut keccak_writer = KeccakWriter::default();
    keccak_writer
        .write_all(raw_json_output.as_bytes())
        .expect("writing to KeccakWriter never fails");

    let KeccakWriter(hash) = keccak_writer;
    Ok(truncated_keccak(<[u8; 32]>::from(hash.finalize())))
}
