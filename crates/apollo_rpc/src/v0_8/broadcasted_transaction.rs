//! This module contains structs for representing a broadcasted transaction.
//!
//! A broadcasted transaction is a transaction that wasn't accepted yet to Starknet.
//!
//! The broadcasted transaction follows the same structure as described in the [`Starknet specs`]
//!
//! [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json

#[cfg(test)]
#[path = "broadcasted_transaction_test.rs"]
mod broadcasted_transaction_test;

use serde::{Deserialize, Serialize};
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::deprecated_contract_class::{
    ContractClassAbiEntry as DeprecatedContractClassAbiEntry,
    EntryPointV0 as DeprecatedEntryPoint,
};
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    Fee,
    PaymasterData,
    Tip,
    TransactionSignature,
};

use super::state::ContractClass;
use super::transaction::{DeployAccountTransaction, InvokeTransaction, ResourceBoundsMapping};

/// Transactions that are ready to be broadcasted to the network and are not included in a block.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BroadcastedTransaction {
    #[serde(rename = "DECLARE")]
    Declare(BroadcastedDeclareTransaction),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(DeployAccountTransaction),
    #[serde(rename = "INVOKE")]
    Invoke(InvokeTransaction),
}

/// A broadcasted declare transaction.
///
/// This transaction is equivalent to the component DECLARE_TXN in the
/// [`Starknet specs`] without the V0 variant and with a contract class (DECLARE_TXN allows having
/// either a contract class or a class hash).
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(tag = "version")]
pub enum BroadcastedDeclareTransaction {
    #[serde(rename = "0x1")]
    V1(BroadcastedDeclareV1Transaction),
    #[serde(rename = "0x2")]
    V2(BroadcastedDeclareV2Transaction),
    #[serde(rename = "0x3")]
    V3(BroadcastedDeclareV3Transaction),
}

/// A broadcasted declare transaction of a Cairo-v0 contract.
///
/// This transaction is equivalent to the component DECLARE_TXN_V1 in the
/// [`Starknet specs`] with a contract class (DECLARE_TXN_V1 allows having either a contract class
/// or a class hash).
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BroadcastedDeclareV1Transaction {
    pub r#type: DeclareType,
    pub contract_class: DeprecatedContractClass,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
}

/// A broadcasted declare transaction of a Cairo-v1 contract.
///
/// This transaction is equivalent to the component DECLARE_TXN_V2 in the
/// [`Starknet specs`] with a contract class (DECLARE_TXN_V2 allows having either a contract class
/// or a class hash).
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct BroadcastedDeclareV2Transaction {
    pub r#type: DeclareType,
    pub contract_class: ContractClass,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BroadcastedDeclareV3Transaction {
    pub r#type: DeclareType,
    pub sender_address: ContractAddress,
    pub compiled_class_hash: CompiledClassHash,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_class: ContractClass,
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

/// The type field of a declare transaction. This enum serializes/deserializes into a constant
/// string.
#[derive(Debug, Deserialize, Serialize, Default, Clone, Copy, Eq, PartialEq)]
pub enum DeclareType {
    #[serde(rename = "DECLARE")]
    #[default]
    Declare,
}

/// A deprecated contract class for Cairo 0 contracts.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeprecatedContractClass {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub abi: Option<Vec<DeprecatedContractClassAbiEntry>>,
    #[serde(rename = "program")]
    pub compressed_program: String,
    pub entry_points_by_type: std::collections::HashMap<EntryPointType, Vec<DeprecatedEntryPoint>>,
}
