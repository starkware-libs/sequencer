#[cfg(test)]
#[path = "rpc_transaction_test.rs"]
mod rpc_transaction_test;

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

use super::common::volition_domain_to_enum_int;
use super::ProtobufConversionError;
use crate::auto_impl_into_and_try_from_vec_u8;
use crate::mempool::RpcTransactionWrapper;
use crate::protobuf::{self};
use crate::transaction::DeclareTransactionV3Common;
auto_impl_into_and_try_from_vec_u8!(RpcTransactionWrapper, protobuf::MempoolTransaction);

impl TryFrom<protobuf::MempoolTransaction> for RpcTransactionWrapper {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::MempoolTransaction) -> Result<Self, Self::Error> {
        Ok(RpcTransactionWrapper(RpcTransaction::try_from(value)?))
    }
}
impl From<RpcTransactionWrapper> for protobuf::MempoolTransaction {
    fn from(value: RpcTransactionWrapper) -> Self {
        protobuf::MempoolTransaction::from(value.0)
    }
}

impl TryFrom<protobuf::MempoolTransaction> for RpcTransaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::MempoolTransaction) -> Result<Self, Self::Error> {
        let txn = value.txn.ok_or(ProtobufConversionError::MissingField {
            field_description: "RpcTransaction::txn",
        })?;
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

impl TryFrom<protobuf::DeployAccountV3> for RpcDeployAccountTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeployAccountV3) -> Result<Self, Self::Error> {
        let snapi_deploy_account: DeployAccountTransactionV3 = value.try_into()?;
        // This conversion can fail only if the resource_bounds are not AllResources.
        snapi_deploy_account.try_into().map_err(|_| ProtobufConversionError::MissingField {
            field_description: "resource_bounds",
        })
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
        snapi_invoke.try_into().map_err(|_| ProtobufConversionError::MissingField {
            field_description: "resource_bounds",
        })
    }
}

impl From<RpcInvokeTransactionV3> for protobuf::InvokeV3 {
    fn from(value: RpcInvokeTransactionV3) -> Self {
        let snapi_invoke: InvokeTransactionV3 = value.into();
        snapi_invoke.into()
    }
}

impl TryFrom<protobuf::DeclareV3WithClass> for RpcDeclareTransactionV3 {
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
        if let ValidResourceBounds::AllResources(resource_bounds) = common.resource_bounds {
            Ok(Self {
                sender_address: common.sender_address,
                compiled_class_hash: common.compiled_class_hash,
                signature: common.signature,
                nonce: common.nonce,
                contract_class: class,
                resource_bounds,
                tip: common.tip,
                paymaster_data: common.paymaster_data,
                account_deployment_data: common.account_deployment_data,
                nonce_data_availability_mode: common.nonce_data_availability_mode,
                fee_data_availability_mode: common.fee_data_availability_mode,
            })
        } else {
            Err(ProtobufConversionError::WrongEnumVariant {
                type_description: "ValidResourceBounds",
                value_as_str: format!("{:?}", common.resource_bounds),
                expected: "AllResources",
            })
        }
    }
}

impl From<RpcDeclareTransactionV3> for protobuf::DeclareV3WithClass {
    fn from(value: RpcDeclareTransactionV3) -> Self {
        let common = protobuf::DeclareV3Common {
            resource_bounds: Some(value.resource_bounds.into()),
            sender: Some(value.sender_address.into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|signature| signature.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            compiled_class_hash: Some(value.compiled_class_hash.0.into()),
            tip: value.tip.0,
            paymaster_data: value.paymaster_data.0.into_iter().map(|data| data.into()).collect(),
            account_deployment_data: value
                .account_deployment_data
                .0
                .into_iter()
                .map(|data| data.into())
                .collect(),
            nonce_data_availability_mode: volition_domain_to_enum_int(
                value.nonce_data_availability_mode,
            ),
            fee_data_availability_mode: volition_domain_to_enum_int(
                value.fee_data_availability_mode,
            ),
        };
        Self { common: Some(common), class: Some(value.contract_class.into()) }
    }
}

impl TryFrom<protobuf::ResourceBounds> for AllResourceBounds {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ResourceBounds) -> Result<Self, Self::Error> {
        Ok(Self {
            l1_gas: value
                .l1_gas
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "ResourceBounds::l1_gas",
                })?
                .try_into()?,
            l2_gas: value
                .l2_gas
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "ResourceBounds::l2_gas",
                })?
                .try_into()?,
            l1_data_gas: value
                .l1_data_gas
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "ResourceBounds::l1_data_gas",
                })?
                .try_into()?,
        })
    }
}

impl From<AllResourceBounds> for protobuf::ResourceBounds {
    fn from(value: AllResourceBounds) -> Self {
        ValidResourceBounds::AllResources(value).into()
    }
}
