use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::rpc_transaction::EntryPointByType;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionSignature,
};

// TODO(Yael): remove the deprecated_gateway_transaction once we decide to support only transactions
// in the Rpc spec format.

#[cfg(test)]
#[path = "deprecated_gateway_transaction_test.rs"]
mod deprecated_gateway_transaction_test;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
#[serde(tag = "type")]
#[serde(deny_unknown_fields)]
pub enum DeprecatedGatewayTransactionV3 {
    #[serde(rename = "DECLARE")]
    Declare(DeprecatedGatewayDeclareTransaction),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(DeprecatedGatewayDeployAccountTransaction),
    #[serde(rename = "INVOKE_FUNCTION")]
    Invoke(DeprecatedGatewayInvokeTransaction),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
#[serde(tag = "version")]
pub enum DeprecatedGatewayInvokeTransaction {
    #[serde(rename = "0x3")]
    V3(DeprecatedGatewayInvokeTransactionV3),
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DeprecatedGatewayInvokeTransactionV3 {
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub resource_bounds: DeprecatedGatewayAllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
#[serde(tag = "version")]
pub enum DeprecatedGatewayDeployAccountTransaction {
    #[serde(rename = "0x3")]
    V3(DeprecatedGatewayDeployAccountTransactionV3),
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DeprecatedGatewayDeployAccountTransactionV3 {
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub resource_bounds: DeprecatedGatewayAllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
#[serde(tag = "version")]
pub enum DeprecatedGatewayDeclareTransaction {
    #[serde(rename = "0x3")]
    V3(DeprecatedGatewayDeclareTransactionV3),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub struct DeprecatedGatewayDeclareTransactionV3 {
    pub sender_address: ContractAddress,
    pub compiled_class_hash: CompiledClassHash,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_class: DeprecatedGatewaySierraContractClass,
    pub resource_bounds: DeprecatedGatewayAllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub struct DeprecatedGatewaySierraContractClass {
    pub sierra_program: String,
    pub contract_class_version: String,
    pub entry_points_by_type: EntryPointByType,
    pub abi: String,
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct DeprecatedGatewayAllResourceBounds {
    pub l1_gas: ResourceBounds,
    pub l2_gas: ResourceBounds,
    pub l1_data_gas: ResourceBounds,
}
