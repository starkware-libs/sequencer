#[cfg(test)]
#[path = "rpc_transaction_test.rs"]
mod rpc_transaction_test;
use prost::Message;
use starknet_api::core::{CompiledClassHash, Nonce};
use starknet_api::rpc_transaction::{
    ContractClass,
    EntryPointByType,
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::state::EntryPoint;
use starknet_api::transaction::{
    AccountDeploymentData,
    AllResourceBounds,
    DeployAccountTransactionV3,
    InvokeTransactionV3,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_types_core::felt::Felt;

use super::common::{enum_int_to_volition_domain, volition_domain_to_enum_int};
use super::ProtobufConversionError;
use crate::auto_impl_into_and_try_from_vec_u8;
use crate::mempool::Broadcasted;
use crate::protobuf::{self, Felt252};

auto_impl_into_and_try_from_vec_u8!(Broadcasted<RpcTransaction>, protobuf::RpcTransaction);

impl TryFrom<protobuf::RpcTransaction> for Broadcasted<RpcTransaction> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::RpcTransaction) -> Result<Self, Self::Error> {
        Ok(Broadcasted(RpcTransaction::try_from(value)?))
    }
}
impl From<Broadcasted<RpcTransaction>> for protobuf::RpcTransaction {
    fn from(value: Broadcasted<RpcTransaction>) -> Self {
        protobuf::RpcTransaction::from(value.0)
    }
}

impl TryFrom<protobuf::RpcTransaction> for RpcTransaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::RpcTransaction) -> Result<Self, Self::Error> {
        let txn = value.txn.ok_or(ProtobufConversionError::MissingField {
            field_description: "RpcTransaction::txn",
        })?;
        Ok(match txn {
            protobuf::rpc_transaction::Txn::DeclareV3(txn) => {
                RpcTransaction::Declare(RpcDeclareTransaction::V3(txn.try_into()?))
            }
            protobuf::rpc_transaction::Txn::DeployAccountV3(txn) => {
                RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(txn.try_into()?))
            }
            protobuf::rpc_transaction::Txn::InvokeV3(txn) => {
                RpcTransaction::Invoke(RpcInvokeTransaction::V3(txn.try_into()?))
            }
        })
    }
}

impl From<RpcTransaction> for protobuf::RpcTransaction {
    fn from(value: RpcTransaction) -> Self {
        match value {
            RpcTransaction::Declare(RpcDeclareTransaction::V3(txn)) => protobuf::RpcTransaction {
                txn: Some(protobuf::rpc_transaction::Txn::DeclareV3(txn.into())),
            },
            RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(txn)) => {
                protobuf::RpcTransaction {
                    txn: Some(protobuf::rpc_transaction::Txn::DeployAccountV3(txn.into())),
                }
            }
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(txn)) => protobuf::RpcTransaction {
                txn: Some(protobuf::rpc_transaction::Txn::InvokeV3(txn.into())),
            },
        }
    }
}

impl TryFrom<protobuf::rpc_transaction::DeclareV3> for RpcDeclareTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::rpc_transaction::DeclareV3) -> Result<Self, Self::Error> {
        let declare_v3 = value.declare_v3.ok_or(ProtobufConversionError::MissingField {
            field_description: "DeclareV3::declare_v3",
        })?;
        let contract_class = value
            .contract_class
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV3::contract_class",
            })?
            .try_into()?;

        let sender_address = declare_v3
            .sender
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV3::sender_address",
            })?
            .try_into()?;
        let compiled_class_hash = CompiledClassHash(
            declare_v3
                .compiled_class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV3::compiled_class_hash",
                })?
                .try_into()?,
        );
        let signature = TransactionSignature(
            declare_v3
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV3::signature",
                })?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );
        let nonce = Nonce(
            declare_v3
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV3::nonce",
                })?
                .try_into()?,
        );
        let resource_bounds = AllResourceBounds::try_from(declare_v3.resource_bounds.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeclareV3::resource_bounds",
            },
        )?)?;
        let tip = Tip(declare_v3.tip);
        let paymaster_data = PaymasterData(
            declare_v3
                .paymaster_data
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );
        let account_deployment_data = AccountDeploymentData(
            declare_v3
                .account_deployment_data
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );
        let nonce_data_availability_mode =
            enum_int_to_volition_domain(declare_v3.nonce_data_availability_mode)?;

        let fee_data_availability_mode =
            enum_int_to_volition_domain(declare_v3.fee_data_availability_mode)?;

        Ok(Self {
            sender_address,
            compiled_class_hash,
            signature,
            nonce,
            contract_class,
            resource_bounds,
            tip,
            paymaster_data,
            account_deployment_data,
            nonce_data_availability_mode,
            fee_data_availability_mode,
        })
    }
}

impl From<RpcDeclareTransactionV3> for protobuf::rpc_transaction::DeclareV3 {
    fn from(value: RpcDeclareTransactionV3) -> Self {
        Self {
            contract_class: Some(value.contract_class.into()),
            declare_v3: Some(protobuf::transaction::DeclareV3 {
                sender: Some(value.sender_address.into()),
                compiled_class_hash: Some(value.compiled_class_hash.0.into()),
                class_hash: None,
                signature: Some(protobuf::AccountSignature {
                    parts: value
                        .signature
                        .0
                        .into_iter()
                        .map(|stark_felt| stark_felt.into())
                        .collect(),
                }),
                nonce: Some(value.nonce.0.into()),
                resource_bounds: Some(protobuf::ResourceBounds::from(value.resource_bounds)),
                tip: value.tip.0,
                paymaster_data: value
                    .paymaster_data
                    .0
                    .iter()
                    .map(|paymaster_data| (*paymaster_data).into())
                    .collect(),
                account_deployment_data: value
                    .account_deployment_data
                    .0
                    .iter()
                    .map(|account_deployment_data| (*account_deployment_data).into())
                    .collect(),
                nonce_data_availability_mode: volition_domain_to_enum_int(
                    value.nonce_data_availability_mode,
                ),
                fee_data_availability_mode: volition_domain_to_enum_int(
                    value.fee_data_availability_mode,
                ),
            }),
        }
    }
}

impl TryFrom<protobuf::transaction::DeployAccountV3> for RpcDeployAccountTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeployAccountV3) -> Result<Self, Self::Error> {
        let resource_bounds = value
            .resource_bounds
            .clone()
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeployAccountV3::resource_bounds",
            })?
            .try_into()?;
        let DeployAccountTransactionV3 {
            resource_bounds: _,
            tip,
            signature,
            nonce,
            class_hash,
            contract_address_salt,
            constructor_calldata,
            nonce_data_availability_mode,
            fee_data_availability_mode,
            paymaster_data,
        } = value.try_into()?;

        Ok(Self {
            resource_bounds,
            tip,
            signature,
            nonce,
            class_hash,
            contract_address_salt,
            constructor_calldata,
            nonce_data_availability_mode,
            fee_data_availability_mode,
            paymaster_data,
        })
    }
}

impl From<RpcDeployAccountTransactionV3> for protobuf::transaction::DeployAccountV3 {
    fn from(value: RpcDeployAccountTransactionV3) -> Self {
        Self {
            resource_bounds: Some(protobuf::ResourceBounds::from(value.resource_bounds)),
            tip: value.tip.0,
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|stark_felt| stark_felt.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            class_hash: Some(value.class_hash.0.into()),
            address_salt: Some(value.contract_address_salt.0.into()),
            calldata: value
                .constructor_calldata
                .0
                .iter()
                .map(|calldata| (*calldata).into())
                .collect(),
            nonce_data_availability_mode: volition_domain_to_enum_int(
                value.nonce_data_availability_mode,
            ),
            fee_data_availability_mode: volition_domain_to_enum_int(
                value.fee_data_availability_mode,
            ),
            paymaster_data: value
                .paymaster_data
                .0
                .iter()
                .map(|paymaster_data| (*paymaster_data).into())
                .collect(),
        }
    }
}

impl TryFrom<protobuf::transaction::InvokeV3> for RpcInvokeTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::InvokeV3) -> Result<Self, Self::Error> {
        let resource_bounds = value
            .resource_bounds
            .clone()
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "InvokeV3::resource_bounds",
            })?
            .try_into()?;
        let InvokeTransactionV3 {
            resource_bounds: _,
            tip,
            signature,
            nonce,
            sender_address,
            calldata,
            nonce_data_availability_mode,
            fee_data_availability_mode,
            paymaster_data,
            account_deployment_data,
        } = value.try_into()?;
        Ok(Self {
            resource_bounds,
            tip,
            signature,
            nonce,
            sender_address,
            calldata,
            nonce_data_availability_mode,
            fee_data_availability_mode,
            paymaster_data,
            account_deployment_data,
        })
    }
}

impl From<RpcInvokeTransactionV3> for protobuf::transaction::InvokeV3 {
    fn from(value: RpcInvokeTransactionV3) -> Self {
        Self {
            resource_bounds: Some(protobuf::ResourceBounds::from(value.resource_bounds)),
            tip: value.tip.0,
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|stark_felt| stark_felt.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            sender: Some(value.sender_address.into()),
            calldata: value.calldata.0.iter().map(|calldata| (*calldata).into()).collect(),
            nonce_data_availability_mode: volition_domain_to_enum_int(
                value.nonce_data_availability_mode,
            ),
            fee_data_availability_mode: volition_domain_to_enum_int(
                value.fee_data_availability_mode,
            ),
            paymaster_data: value
                .paymaster_data
                .0
                .iter()
                .map(|paymaster_data| (*paymaster_data).into())
                .collect(),
            account_deployment_data: value
                .account_deployment_data
                .0
                .iter()
                .map(|account_deployment_data| (*account_deployment_data).into())
                .collect(),
        }
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

impl TryFrom<protobuf::Cairo1Class> for ContractClass {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Cairo1Class) -> Result<Self, Self::Error> {
        let sierra_program =
            value.program.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;
        let contract_class_version = value.contract_class_version;
        let entry_points = value.entry_points.ok_or(ProtobufConversionError::MissingField {
            field_description: "Cairo1Class::entry_points_by_type",
        })?;
        let entry_points_by_type = EntryPointByType {
            constructor: entry_points
                .constructors
                .into_iter()
                .map(EntryPoint::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            external: entry_points
                .externals
                .into_iter()
                .map(EntryPoint::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            l1handler: entry_points
                .l1_handlers
                .into_iter()
                .map(EntryPoint::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        };
        let abi = value.abi;
        Ok(Self { sierra_program, contract_class_version, entry_points_by_type, abi })
    }
}

impl From<ContractClass> for protobuf::Cairo1Class {
    fn from(value: ContractClass) -> Self {
        let program = value.sierra_program.into_iter().map(Felt252::from).collect();
        let contract_class_version = value.contract_class_version;
        let entry_points = protobuf::Cairo1EntryPoints {
            constructors: value
                .entry_points_by_type
                .constructor
                .into_iter()
                .map(protobuf::SierraEntryPoint::from)
                .collect(),
            externals: value
                .entry_points_by_type
                .external
                .into_iter()
                .map(protobuf::SierraEntryPoint::from)
                .collect(),
            l1_handlers: value
                .entry_points_by_type
                .l1handler
                .into_iter()
                .map(protobuf::SierraEntryPoint::from)
                .collect(),
        };
        let abi = value.abi;
        Self { program, contract_class_version, entry_points: Some(entry_points), abi }
    }
}
