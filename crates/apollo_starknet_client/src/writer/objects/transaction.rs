//! This module contains all the different transactions that can be added to [`Starknet`] via the
//! gateway.
//!
//! Each transaction can be serialized into a JSON object that the gateway can receive through the
//! `add_transaction` HTTP method.
//!
//! [`Starknet`]: https://starknet.io/

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::contract_class::EntryPointType;
use starknet_api::deprecated_contract_class::{
    ContractClassAbiEntry as DeprecatedContractClassAbiEntry,
    EntryPointV0 as DeprecatedEntryPoint,
};
use starknet_api::state::EntryPoint;

// The structs that are implemented here are the structs that have deviations from starknet_api.

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeprecatedContractClass {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub abi: Option<Vec<DeprecatedContractClassAbiEntry>>,
    #[serde(rename = "program")]
    // TODO(shahak): Create a struct for a compressed base64 value.
    pub compressed_program: String,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<DeprecatedEntryPoint>>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    // TODO(shahak): Create a struct for a compressed base64 value.
    #[serde(rename = "sierra_program")]
    pub compressed_sierra_program: String,
    pub contract_class_version: String,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    pub abi: String,
}
