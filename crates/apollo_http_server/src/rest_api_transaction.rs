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
use starknet_api::transaction::TransactionVersion;

#[cfg(test)]
#[path = "rest_api_transaction_test.rs"]
mod rest_api_transaction_test;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
#[serde(tag = "type")]
#[serde(deny_unknown_fields)]
pub enum RestTransactionV3 {
    #[serde(rename = "DECLARE")]
    Declare(RestDeclareTransactionV3),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(RestDeployAccountTransactionV3),
    #[serde(rename = "INVOKE_FUNCTION")]
    Invoke(RestInvokeTransactionV3),
}

impl From<RestTransactionV3> for RpcTransaction {
    fn from(rest_tx: RestTransactionV3) -> Self {
        match rest_tx {
            RestTransactionV3::Declare(rest_declare_tx) => {
                RpcTransaction::Declare(RpcDeclareTransaction::V3(rest_declare_tx.into()))
            }
            RestTransactionV3::DeployAccount(rest_deploy_account_tx) => {
                RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(
                    rest_deploy_account_tx.into(),
                ))
            }
            RestTransactionV3::Invoke(rest_invoke_tx) => {
                RpcTransaction::Invoke(RpcInvokeTransaction::V3(rest_invoke_tx.into()))
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RestInvokeTransactionV3 {
    pub version: TransactionVersion,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub resource_bounds: RestAllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

impl From<RestInvokeTransactionV3> for RpcInvokeTransactionV3 {
    fn from(rest_invoke_tx: RestInvokeTransactionV3) -> Self {
        RpcInvokeTransactionV3 {
            sender_address: rest_invoke_tx.sender_address,
            calldata: rest_invoke_tx.calldata,
            signature: rest_invoke_tx.signature,
            nonce: rest_invoke_tx.nonce,
            resource_bounds: rest_invoke_tx.resource_bounds.into(),
            tip: rest_invoke_tx.tip,
            paymaster_data: rest_invoke_tx.paymaster_data,
            account_deployment_data: rest_invoke_tx.account_deployment_data,
            nonce_data_availability_mode: rest_invoke_tx.nonce_data_availability_mode,
            fee_data_availability_mode: rest_invoke_tx.fee_data_availability_mode,
        }
    }
}

#[cfg(any(feature = "testing", test))]
impl From<RpcInvokeTransactionV3> for RestInvokeTransactionV3 {
    fn from(value: RpcInvokeTransactionV3) -> Self {
        Self {
            version: TransactionVersion::THREE,
            calldata: value.calldata,
            tip: value.tip,
            resource_bounds: RestAllResourceBounds::from(value.resource_bounds),
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

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RestDeployAccountTransactionV3 {
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub resource_bounds: RestAllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

impl From<RestDeployAccountTransactionV3> for RpcDeployAccountTransactionV3 {
    fn from(rest_deploy_account_tx: RestDeployAccountTransactionV3) -> Self {
        RpcDeployAccountTransactionV3 {
            signature: rest_deploy_account_tx.signature,
            nonce: rest_deploy_account_tx.nonce,
            class_hash: rest_deploy_account_tx.class_hash,
            contract_address_salt: rest_deploy_account_tx.contract_address_salt,
            constructor_calldata: rest_deploy_account_tx.constructor_calldata,
            resource_bounds: rest_deploy_account_tx.resource_bounds.into(),
            tip: rest_deploy_account_tx.tip,
            paymaster_data: rest_deploy_account_tx.paymaster_data,
            nonce_data_availability_mode: rest_deploy_account_tx.nonce_data_availability_mode,
            fee_data_availability_mode: rest_deploy_account_tx.fee_data_availability_mode,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub struct RestDeclareTransactionV3 {
    pub version: TransactionVersion,
    pub sender_address: ContractAddress,
    pub compiled_class_hash: CompiledClassHash,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_class: RestSierraContractClass,
    pub resource_bounds: RestAllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

impl From<RestDeclareTransactionV3> for RpcDeclareTransactionV3 {
    fn from(rest_declare_tx: RestDeclareTransactionV3) -> Self {
        RpcDeclareTransactionV3 {
            sender_address: rest_declare_tx.sender_address,
            compiled_class_hash: rest_declare_tx.compiled_class_hash,
            signature: rest_declare_tx.signature,
            nonce: rest_declare_tx.nonce,
            contract_class: rest_declare_tx.contract_class.into(),
            resource_bounds: rest_declare_tx.resource_bounds.into(),
            tip: rest_declare_tx.tip,
            paymaster_data: rest_declare_tx.paymaster_data,
            account_deployment_data: rest_declare_tx.account_deployment_data,
            nonce_data_availability_mode: rest_declare_tx.nonce_data_availability_mode,
            fee_data_availability_mode: rest_declare_tx.fee_data_availability_mode,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub struct RestSierraContractClass {
    pub sierra_program: String,
    pub contract_class_version: String,
    pub entry_points_by_type: EntryPointByType,
    pub abi: String,
}

impl From<RestSierraContractClass> for SierraContractClass {
    fn from(_rest_sierra_contract_class: RestSierraContractClass) -> Self {
        unimplemented!()
    }
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct RestAllResourceBounds {
    pub l1_gas: ResourceBounds,
    pub l2_gas: ResourceBounds,
    pub l1_data_gas: ResourceBounds,
}

impl From<RestAllResourceBounds> for AllResourceBounds {
    fn from(rest_all_resource_bounds: RestAllResourceBounds) -> Self {
        AllResourceBounds {
            l1_gas: rest_all_resource_bounds.l1_gas,
            l2_gas: rest_all_resource_bounds.l2_gas,
            l1_data_gas: rest_all_resource_bounds.l1_data_gas,
        }
    }
}

#[cfg(any(feature = "testing", test))]
impl From<AllResourceBounds> for RestAllResourceBounds {
    fn from(all_resource_bounds: AllResourceBounds) -> Self {
        RestAllResourceBounds {
            l1_gas: all_resource_bounds.l1_gas,
            l2_gas: all_resource_bounds.l2_gas,
            l1_data_gas: all_resource_bounds.l1_data_gas,
        }
    }
}
