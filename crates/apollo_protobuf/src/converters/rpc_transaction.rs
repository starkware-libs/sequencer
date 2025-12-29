#[cfg(test)]
#[path = "rpc_transaction_test.rs"]
mod rpc_transaction_test;

use std::sync::Arc;

use prost::Message;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::state::SierraContractClass;
use starknet_api::transaction::fields::{AllResourceBounds, ValidResourceBounds};
use starknet_api::transaction::{DeployAccountTransactionV3, InvokeTransactionV3};

use super::common::missing;
use super::ProtobufConversionError;
use crate::auto_impl_into_and_try_from_vec_u8;
use crate::mempool::RpcTransactionBatch;
use crate::protobuf::{self};
use crate::transaction::DeclareTransactionV3Common;
auto_impl_into_and_try_from_vec_u8!(RpcTransactionBatch, protobuf::MempoolTransactionBatch);

const DEPRECATED_RESOURCE_BOUNDS_ERROR: ProtobufConversionError =
    ProtobufConversionError::MissingField { field_description: "ResourceBounds::l1_data_gas" };

impl TryFrom<protobuf::MempoolTransaction> for RpcTransaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::MempoolTransaction) -> Result<Self, Self::Error> {
        let txn = value.txn.ok_or(missing("RpcTransaction::txn"))?;
        Ok(match txn {
            protobuf::mempool_transaction::Txn::DeclareV3(txn) => {
                RpcTransaction::Declare(RpcDeclareTransaction::V3(txn.try_into()?))
            }
            protobuf::mempool_transaction::Txn::DeployAccountV3(txn) => {
                RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(txn.try_into()?))
            }
            protobuf::mempool_transaction::Txn::InvokeV3(txn) => {
                RpcTransaction::Invoke(RpcInvokeTransaction::V3(txn.try_into()?))
            }
        })
    }
}

impl From<RpcTransaction> for protobuf::MempoolTransaction {
    fn from(value: RpcTransaction) -> Self {
        match value {
            RpcTransaction::Declare(RpcDeclareTransaction::V3(txn)) => {
                protobuf::MempoolTransaction {
                    txn: Some(protobuf::mempool_transaction::Txn::DeclareV3(txn.into())),
                    // TODO(alonl): Consider removing transaction hash from protobuf
                    transaction_hash: None,
                }
            }
            RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(txn)) => {
                protobuf::MempoolTransaction {
                    txn: Some(protobuf::mempool_transaction::Txn::DeployAccountV3(txn.into())),
                    // TODO(alonl): Consider removing transaction hash from protobuf
                    transaction_hash: None,
                }
            }
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(txn)) => {
                protobuf::MempoolTransaction {
                    txn: Some(protobuf::mempool_transaction::Txn::InvokeV3(txn.into())),
                    // TODO(alonl): Consider removing transaction hash from protobuf
                    transaction_hash: None,
                }
            }
        }
    }
}

impl TryFrom<protobuf::MempoolTransactionBatch> for RpcTransactionBatch {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::MempoolTransactionBatch) -> Result<Self, Self::Error> {
        Ok(RpcTransactionBatch(
            value
                .transactions
                .into_iter()
                .map(RpcTransaction::try_from)
                .collect::<Result<_, _>>()?,
        ))
    }
}

impl From<RpcTransactionBatch> for protobuf::MempoolTransactionBatch {
    fn from(value: RpcTransactionBatch) -> Self {
        protobuf::MempoolTransactionBatch {
            transactions: value.0.into_iter().map(protobuf::MempoolTransaction::from).collect(),
        }
    }
}

impl TryFrom<protobuf::DeployAccountV3> for RpcDeployAccountTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeployAccountV3) -> Result<Self, Self::Error> {
        let snapi_deploy_account: DeployAccountTransactionV3 = value.try_into()?;
        // This conversion can fail only if the resource_bounds are not AllResources.
        snapi_deploy_account.try_into().map_err(|_| DEPRECATED_RESOURCE_BOUNDS_ERROR)
    }
}

impl From<RpcDeployAccountTransactionV3> for protobuf::DeployAccountV3 {
    fn from(value: RpcDeployAccountTransactionV3) -> Self {
        let snapi_deploy_account: DeployAccountTransactionV3 = value.into();
        snapi_deploy_account.into()
    }
}

impl TryFrom<protobuf::InvokeV3> for RpcInvokeTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::InvokeV3) -> Result<Self, Self::Error> {
        let snapi_invoke: InvokeTransactionV3 = value.try_into()?;
        // This conversion can fail only if the resource_bounds are not AllResources.
        // TODO(AvivG): Currently creates the tx with a default proof, should be populated by the ProofManager.
        snapi_invoke.try_into().map_err(|_| DEPRECATED_RESOURCE_BOUNDS_ERROR)
    }

impl From<RpcInvokeTransactionV3> for protobuf::InvokeV3 {
    fn from(value: RpcInvokeTransactionV3) -> Self {
        let snapi_invoke: InvokeTransactionV3 = value.into();
        snapi_invoke.into()
    }
}

impl TryFrom<protobuf::DeclareV3WithClass> for (DeclareTransactionV3Common, SierraContractClass) {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeclareV3WithClass) -> Result<Self, Self::Error> {
        let common = DeclareTransactionV3Common::try_from(value.common.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeclareV3WithClass::common",
            },
        )?)?;
        let class: SierraContractClass = SierraContractClass::try_from(value.class.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeclareV3WithClass::class",
            },
        )?)?;
        Ok((common, class))
    }
}

impl From<(DeclareTransactionV3Common, SierraContractClass)> for protobuf::DeclareV3WithClass {
    fn from(value: (DeclareTransactionV3Common, SierraContractClass)) -> Self {
        Self { common: Some(value.0.into()), class: Some(value.1.into()) }
    }
}

impl TryFrom<protobuf::DeclareV3WithClass> for RpcDeclareTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeclareV3WithClass) -> Result<Self, Self::Error> {
        let (common, class) = value.try_into()?;
        Ok(Self {
            resource_bounds: match common.resource_bounds {
                ValidResourceBounds::AllResources(resource_bounds) => resource_bounds,
                _ => {
                    return Err(DEPRECATED_RESOURCE_BOUNDS_ERROR);
                }
            },
            sender_address: common.sender_address,
            signature: common.signature,
            nonce: common.nonce,
            compiled_class_hash: common.compiled_class_hash,
            contract_class: class,
            tip: common.tip,
            paymaster_data: common.paymaster_data,
            account_deployment_data: common.account_deployment_data,
            nonce_data_availability_mode: common.nonce_data_availability_mode,
            fee_data_availability_mode: common.fee_data_availability_mode,
        })
    }
}

impl From<RpcDeclareTransactionV3> for protobuf::DeclareV3WithClass {
    fn from(value: RpcDeclareTransactionV3) -> Self {
        let snapi_declare = DeclareTransactionV3Common {
            resource_bounds: ValidResourceBounds::AllResources(value.resource_bounds),
            sender_address: value.sender_address,
            signature: value.signature,
            nonce: value.nonce,
            compiled_class_hash: value.compiled_class_hash,
            tip: value.tip,
            paymaster_data: value.paymaster_data,
            account_deployment_data: value.account_deployment_data,
            nonce_data_availability_mode: value.nonce_data_availability_mode,
            fee_data_availability_mode: value.fee_data_availability_mode,
        };
        (snapi_declare, value.contract_class).into()
    }
}

impl TryFrom<protobuf::ResourceBounds> for AllResourceBounds {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ResourceBounds) -> Result<Self, Self::Error> {
        Ok(Self {
            l1_gas: value.l1_gas.ok_or(missing("ResourceBounds::l1_gas"))?.try_into()?,
            l2_gas: value.l2_gas.ok_or(missing("ResourceBounds::l2_gas"))?.try_into()?,
            l1_data_gas: value
                .l1_data_gas
                .ok_or(missing("ResourceBounds::l1_data_gas"))?
                .try_into()?,
        })
    }
}

impl From<AllResourceBounds> for protobuf::ResourceBounds {
    fn from(value: AllResourceBounds) -> Self {
        ValidResourceBounds::AllResources(value).into()
    }
}
