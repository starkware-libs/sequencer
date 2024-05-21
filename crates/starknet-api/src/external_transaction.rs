#[cfg(test)]
#[path = "external_transaction_test.rs"]
mod external_transaction_test;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use crate::data_availability::DataAvailabilityMode;
use crate::state::{EntryPoint, EntryPointType};
use crate::transaction::{
    AccountDeploymentData, Calldata, ContractAddressSalt, PaymasterData, ResourceBoundsMapping,
    Tip, TransactionSignature,
};

/// An external transaction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum ExternalTransaction {
    /// A declare transaction.
    #[serde(rename = "DECLARE")]
    Declare(ExternalDeclareTransaction),
    /// A deploy account transaction.
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(ExternalDeployAccountTransaction),
    /// An invoke transaction.
    #[serde(rename = "INVOKE_FUNCTION")]
    Invoke(ExternalInvokeTransaction),
}

macro_rules! implement_ref_getters {
    ($(($member_name:ident, $member_type:ty)), *) => {
        $(pub fn $member_name(&self) -> &$member_type {
            match self {
                ExternalTransaction::Declare(
                    ExternalDeclareTransaction::V3(tx)
                ) => &tx.$member_name,
                ExternalTransaction::DeployAccount(
                    ExternalDeployAccountTransaction::V3(tx)
                ) => &tx.$member_name,
                ExternalTransaction::Invoke(
                    ExternalInvokeTransaction::V3(tx)
                ) => &tx.$member_name
            }
        })*
    };
}

impl ExternalTransaction {
    implement_ref_getters!(
        (resource_bounds, ResourceBoundsMapping),
        (signature, TransactionSignature)
    );
}

/// A declare transaction that can be added to Starknet through the Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "version")]
pub enum ExternalDeclareTransaction {
    #[serde(rename = "0x3")]
    V3(ExternalDeclareTransactionV3),
}

/// A deploy account transaction that can be added to Starknet through the Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "version")]
pub enum ExternalDeployAccountTransaction {
    #[serde(rename = "0x3")]
    V3(ExternalDeployAccountTransactionV3),
}

/// An invoke transaction that can be added to Starknet through the Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "version")]
pub enum ExternalInvokeTransaction {
    #[serde(rename = "0x3")]
    V3(ExternalInvokeTransactionV3),
}

/// A declare transaction of a Cairo-v1 contract class that can be added to Starknet through the
/// Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalDeclareTransactionV3 {
    pub contract_class: ContractClass,
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
}

/// A deploy account transaction that can be added to Starknet through the Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalDeployAccountTransactionV3 {
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: Calldata,
    pub nonce: Nonce,
    pub signature: TransactionSignature,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
}

/// An invoke account transaction that can be added to Starknet through the Starknet gateway.
/// The invoke is a V3 transaction.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalInvokeTransactionV3 {
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub calldata: Calldata,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub signature: TransactionSignature,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContractClass {
    #[serde(rename = "sierra_program")]
    pub compressed_sierra_program: String,
    pub contract_class_version: String,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    pub abi: String,
}
