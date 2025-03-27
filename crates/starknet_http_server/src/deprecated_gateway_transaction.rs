use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::rpc_transaction::{
    EntryPointByType,
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::state::SierraContractClass;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
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

impl From<DeprecatedGatewayTransactionV3> for RpcTransaction {
    fn from(deprecated_tx: DeprecatedGatewayTransactionV3) -> Self {
        match deprecated_tx {
            DeprecatedGatewayTransactionV3::Declare(DeprecatedGatewayDeclareTransaction::V3(
                deprecated_declare_tx,
            )) => RpcTransaction::Declare(RpcDeclareTransaction::V3(deprecated_declare_tx.into())),
            DeprecatedGatewayTransactionV3::DeployAccount(
                DeprecatedGatewayDeployAccountTransaction::V3(deprecated_deploy_account_tx),
            ) => RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(
                deprecated_deploy_account_tx.into(),
            )),
            DeprecatedGatewayTransactionV3::Invoke(DeprecatedGatewayInvokeTransaction::V3(
                deprecated_invoke_tx,
            )) => RpcTransaction::Invoke(RpcInvokeTransaction::V3(deprecated_invoke_tx.into())),
        }
    }
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

impl From<DeprecatedGatewayInvokeTransactionV3> for RpcInvokeTransactionV3 {
    fn from(deprecated_invoke_tx: DeprecatedGatewayInvokeTransactionV3) -> Self {
        RpcInvokeTransactionV3 {
            sender_address: deprecated_invoke_tx.sender_address,
            calldata: deprecated_invoke_tx.calldata,
            signature: deprecated_invoke_tx.signature,
            nonce: deprecated_invoke_tx.nonce,
            resource_bounds: deprecated_invoke_tx.resource_bounds.into(),
            tip: deprecated_invoke_tx.tip,
            paymaster_data: deprecated_invoke_tx.paymaster_data,
            account_deployment_data: deprecated_invoke_tx.account_deployment_data,
            nonce_data_availability_mode: deprecated_invoke_tx.nonce_data_availability_mode,
            fee_data_availability_mode: deprecated_invoke_tx.fee_data_availability_mode,
        }
    }
}

#[cfg(any(feature = "testing", test))]
impl From<RpcInvokeTransactionV3> for DeprecatedGatewayInvokeTransactionV3 {
    fn from(value: RpcInvokeTransactionV3) -> Self {
        Self {
            calldata: value.calldata,
            tip: value.tip,
            resource_bounds: DeprecatedGatewayAllResourceBounds::from(value.resource_bounds),
            paymaster_data: value.paymaster_data,
            sender_address: value.sender_address,
            signature: value.signature,
            nonce: value.nonce,
            account_deployment_data: value.account_deployment_data,
            nonce_data_availability_mode: value.nonce_data_availability_mode,
            fee_data_availability_mode: value.fee_data_availability_mode,
        }
    }
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

impl From<DeprecatedGatewayDeployAccountTransactionV3> for RpcDeployAccountTransactionV3 {
    fn from(deprecated_deploy_account_tx: DeprecatedGatewayDeployAccountTransactionV3) -> Self {
        RpcDeployAccountTransactionV3 {
            signature: deprecated_deploy_account_tx.signature,
            nonce: deprecated_deploy_account_tx.nonce,
            class_hash: deprecated_deploy_account_tx.class_hash,
            contract_address_salt: deprecated_deploy_account_tx.contract_address_salt,
            constructor_calldata: deprecated_deploy_account_tx.constructor_calldata,
            resource_bounds: deprecated_deploy_account_tx.resource_bounds.into(),
            tip: deprecated_deploy_account_tx.tip,
            paymaster_data: deprecated_deploy_account_tx.paymaster_data,
            nonce_data_availability_mode: deprecated_deploy_account_tx.nonce_data_availability_mode,
            fee_data_availability_mode: deprecated_deploy_account_tx.fee_data_availability_mode,
        }
    }
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

impl From<DeprecatedGatewayDeclareTransactionV3> for RpcDeclareTransactionV3 {
    fn from(deprecated_declare_tx: DeprecatedGatewayDeclareTransactionV3) -> Self {
        RpcDeclareTransactionV3 {
            sender_address: deprecated_declare_tx.sender_address,
            compiled_class_hash: deprecated_declare_tx.compiled_class_hash,
            signature: deprecated_declare_tx.signature,
            nonce: deprecated_declare_tx.nonce,
            contract_class: deprecated_declare_tx.contract_class.into(),
            resource_bounds: deprecated_declare_tx.resource_bounds.into(),
            tip: deprecated_declare_tx.tip,
            paymaster_data: deprecated_declare_tx.paymaster_data,
            account_deployment_data: deprecated_declare_tx.account_deployment_data,
            nonce_data_availability_mode: deprecated_declare_tx.nonce_data_availability_mode,
            fee_data_availability_mode: deprecated_declare_tx.fee_data_availability_mode,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub struct DeprecatedGatewaySierraContractClass {
    pub sierra_program: String,
    pub contract_class_version: String,
    pub entry_points_by_type: EntryPointByType,
    pub abi: String,
}

impl From<DeprecatedGatewaySierraContractClass> for SierraContractClass {
    fn from(_deprecated_sierra_contract_class: DeprecatedGatewaySierraContractClass) -> Self {
        unimplemented!()
    }
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

impl From<DeprecatedGatewayAllResourceBounds> for AllResourceBounds {
    fn from(deprecated_all_resource_bounds: DeprecatedGatewayAllResourceBounds) -> Self {
        AllResourceBounds {
            l1_gas: deprecated_all_resource_bounds.l1_gas,
            l2_gas: deprecated_all_resource_bounds.l2_gas,
            l1_data_gas: deprecated_all_resource_bounds.l1_data_gas,
        }
    }
}

#[cfg(any(feature = "testing", test))]
impl From<AllResourceBounds> for DeprecatedGatewayAllResourceBounds {
    fn from(all_resource_bounds: AllResourceBounds) -> Self {
        DeprecatedGatewayAllResourceBounds {
            l1_gas: all_resource_bounds.l1_gas,
            l2_gas: all_resource_bounds.l2_gas,
            l1_data_gas: all_resource_bounds.l1_data_gas,
        }
    }
}
