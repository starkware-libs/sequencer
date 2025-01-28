use starknet_api::core::{ClassHash, CompiledClassHash, EntryPointSelector, Nonce};
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    Fee,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::{
    DeclareTransaction,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeclareTransactionV3,
    DeployAccountTransaction,
    DeployAccountTransactionV1,
    DeployAccountTransactionV3,
    DeployTransaction,
    InvokeTransaction,
    InvokeTransactionV0,
    InvokeTransactionV1,
    InvokeTransactionV3,
    L1HandlerTransaction,
    Transaction,
    TransactionVersion,
};
use starknet_types_core::felt::Felt;

use super::common::{
    enum_int_to_volition_domain,
    try_from_starkfelt_to_u128,
    try_from_starkfelt_to_u32,
    volition_domain_to_enum_int,
};
use super::ProtobufConversionError;
use crate::protobuf;

impl TryFrom<protobuf::Transaction> for Transaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Transaction) -> Result<Self, Self::Error> {
        let txn = value.txn.ok_or(ProtobufConversionError::MissingField {
            field_description: "Transaction::txn",
        })?;
        Ok(match txn {
            protobuf::transaction::Txn::DeclareV0(declare_v0) => Transaction::Declare(
                DeclareTransaction::V0(DeclareTransactionV0V1::try_from(declare_v0)?),
            ),
            protobuf::transaction::Txn::DeclareV1(declare_v1) => Transaction::Declare(
                DeclareTransaction::V1(DeclareTransactionV0V1::try_from(declare_v1)?),
            ),
            protobuf::transaction::Txn::DeclareV2(declare_v2) => Transaction::Declare(
                DeclareTransaction::V2(DeclareTransactionV2::try_from(declare_v2)?),
            ),
            protobuf::transaction::Txn::DeclareV3(declare_v3) => Transaction::Declare(
                DeclareTransaction::V3(DeclareTransactionV3::try_from(declare_v3)?),
            ),
            protobuf::transaction::Txn::Deploy(deploy) => {
                Transaction::Deploy(DeployTransaction::try_from(deploy)?)
            }
            protobuf::transaction::Txn::DeployAccountV1(deploy_account_v1) => {
                Transaction::DeployAccount(DeployAccountTransaction::V1(
                    DeployAccountTransactionV1::try_from(deploy_account_v1)?,
                ))
            }
            protobuf::transaction::Txn::DeployAccountV3(deploy_account_v3) => {
                Transaction::DeployAccount(DeployAccountTransaction::V3(
                    DeployAccountTransactionV3::try_from(deploy_account_v3)?,
                ))
            }
            protobuf::transaction::Txn::InvokeV0(invoke_v0) => Transaction::Invoke(
                InvokeTransaction::V0(InvokeTransactionV0::try_from(invoke_v0)?),
            ),
            protobuf::transaction::Txn::InvokeV1(invoke_v1) => Transaction::Invoke(
                InvokeTransaction::V1(InvokeTransactionV1::try_from(invoke_v1)?),
            ),
            protobuf::transaction::Txn::InvokeV3(invoke_v3) => Transaction::Invoke(
                InvokeTransaction::V3(InvokeTransactionV3::try_from(invoke_v3)?),
            ),
            protobuf::transaction::Txn::L1Handler(l1_handler) => {
                Transaction::L1Handler(L1HandlerTransaction::try_from(l1_handler)?)
            }
        })
    }
}

impl From<Transaction> for protobuf::Transaction {
    fn from(value: Transaction) -> Self {
        match value {
            Transaction::Declare(DeclareTransaction::V0(declare_v0)) => protobuf::Transaction {
                txn: Some(protobuf::transaction::Txn::DeclareV0(declare_v0.into())),
                transaction_hash: None,
            },
            Transaction::Declare(DeclareTransaction::V1(declare_v1)) => protobuf::Transaction {
                txn: Some(protobuf::transaction::Txn::DeclareV1(declare_v1.into())),
                transaction_hash: None,
            },
            Transaction::Declare(DeclareTransaction::V2(declare_v2)) => protobuf::Transaction {
                txn: Some(protobuf::transaction::Txn::DeclareV2(declare_v2.into())),
                transaction_hash: None,
            },
            Transaction::Declare(DeclareTransaction::V3(declare_v3)) => protobuf::Transaction {
                txn: Some(protobuf::transaction::Txn::DeclareV3(declare_v3.into())),
                transaction_hash: None,
            },
            Transaction::Deploy(deploy) => protobuf::Transaction {
                txn: Some(protobuf::transaction::Txn::Deploy(deploy.into())),
                transaction_hash: None,
            },
            Transaction::DeployAccount(deploy_account) => match deploy_account {
                DeployAccountTransaction::V1(deploy_account_v1) => protobuf::Transaction {
                    txn: Some(protobuf::transaction::Txn::DeployAccountV1(
                        deploy_account_v1.into(),
                    )),
                    transaction_hash: None,
                },
                DeployAccountTransaction::V3(deploy_account_v3) => protobuf::Transaction {
                    txn: Some(protobuf::transaction::Txn::DeployAccountV3(
                        deploy_account_v3.into(),
                    )),
                    transaction_hash: None,
                },
            },
            Transaction::Invoke(invoke) => match invoke {
                InvokeTransaction::V0(invoke_v0) => protobuf::Transaction {
                    txn: Some(protobuf::transaction::Txn::InvokeV0(invoke_v0.into())),
                    transaction_hash: None,
                },
                InvokeTransaction::V1(invoke_v1) => protobuf::Transaction {
                    txn: Some(protobuf::transaction::Txn::InvokeV1(invoke_v1.into())),
                    transaction_hash: None,
                },
                InvokeTransaction::V3(invoke_v3) => protobuf::Transaction {
                    txn: Some(protobuf::transaction::Txn::InvokeV3(invoke_v3.into())),
                    transaction_hash: None,
                },
            },
            Transaction::L1Handler(l1_handler) => protobuf::Transaction {
                txn: Some(protobuf::transaction::Txn::L1Handler(l1_handler.into())),
                transaction_hash: None,
            },
        }
    }
}

impl TryFrom<protobuf::transaction::DeployAccountV1> for DeployAccountTransactionV1 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeployAccountV1) -> Result<Self, Self::Error> {
        let max_fee_felt =
            Felt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "DeployAccountV1::max_fee",
            })?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV1::signature",
                })?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        let nonce = Nonce(
            value
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV1::nonce",
                })?
                .try_into()?,
        );

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV1::class_hash",
                })?
                .try_into()?,
        );

        let contract_address_salt = ContractAddressSalt(
            value
                .address_salt
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV1::address_salt",
                })?
                .try_into()?,
        );

        let constructor_calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let constructor_calldata = Calldata(constructor_calldata.into());

        Ok(Self {
            max_fee,
            signature,
            nonce,
            class_hash,
            contract_address_salt,
            constructor_calldata,
        })
    }
}

impl From<DeployAccountTransactionV1> for protobuf::transaction::DeployAccountV1 {
    fn from(value: DeployAccountTransactionV1) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
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
        }
    }
}

impl TryFrom<protobuf::transaction::DeployAccountV3> for DeployAccountTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeployAccountV3) -> Result<Self, Self::Error> {
        let resource_bounds = ValidResourceBounds::try_from(value.resource_bounds.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeployAccountV3::resource_bounds",
            },
        )?)?;

        let tip = Tip(value.tip);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV3::signature",
                })?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        let nonce = Nonce(
            value
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV3::nonce",
                })?
                .try_into()?,
        );

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV3::class_hash",
                })?
                .try_into()?,
        );

        let contract_address_salt = ContractAddressSalt(
            value
                .address_salt
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV3::address_salt",
                })?
                .try_into()?,
        );

        let constructor_calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let constructor_calldata = Calldata(constructor_calldata.into());

        let nonce_data_availability_mode =
            enum_int_to_volition_domain(value.nonce_data_availability_mode)?;

        let fee_data_availability_mode =
            enum_int_to_volition_domain(value.fee_data_availability_mode)?;

        let paymaster_data = PaymasterData(
            value.paymaster_data.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?,
        );

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

impl From<DeployAccountTransactionV3> for protobuf::transaction::DeployAccountV3 {
    fn from(value: DeployAccountTransactionV3) -> Self {
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

impl TryFrom<protobuf::transaction::InvokeV0> for InvokeTransactionV0 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::InvokeV0) -> Result<Self, Self::Error> {
        let max_fee_felt =
            Felt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "InvokeV0::max_fee",
            })?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "InvokeV0::signature",
                })?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        let contract_address = value
            .address
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "InvokeV0::address",
            })?
            .try_into()?;

        let entry_point_selector_felt = Felt::try_from(value.entry_point_selector.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "InvokeV0::entry_point_selector",
            },
        )?)?;
        let entry_point_selector = EntryPointSelector(entry_point_selector_felt);

        let calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let calldata = Calldata(calldata.into());

        Ok(Self { max_fee, signature, contract_address, entry_point_selector, calldata })
    }
}

impl From<InvokeTransactionV0> for protobuf::transaction::InvokeV0 {
    fn from(value: InvokeTransactionV0) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|stark_felt| stark_felt.into()).collect(),
            }),
            address: Some(value.contract_address.into()),
            entry_point_selector: Some(value.entry_point_selector.0.into()),
            calldata: value.calldata.0.iter().map(|calldata| (*calldata).into()).collect(),
        }
    }
}

impl TryFrom<protobuf::transaction::InvokeV1> for InvokeTransactionV1 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::InvokeV1) -> Result<Self, Self::Error> {
        let max_fee_felt =
            Felt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "InvokeV1::max_fee",
            })?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "InvokeV1::signature",
                })?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField { field_description: "InvokeV1::sender" })?
            .try_into()?;

        let nonce = Nonce(
            value
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "InvokeV1::nonce",
                })?
                .try_into()?,
        );

        let calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let calldata = Calldata(calldata.into());

        Ok(Self { max_fee, signature, nonce, sender_address, calldata })
    }
}

impl From<InvokeTransactionV1> for protobuf::transaction::InvokeV1 {
    fn from(value: InvokeTransactionV1) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|signature| signature.into()).collect(),
            }),
            sender: Some(value.sender_address.into()),
            nonce: Some(value.nonce.0.into()),
            calldata: value.calldata.0.iter().map(|calldata| (*calldata).into()).collect(),
        }
    }
}

impl TryFrom<protobuf::transaction::InvokeV3> for InvokeTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::InvokeV3) -> Result<Self, Self::Error> {
        let resource_bounds = ValidResourceBounds::try_from(value.resource_bounds.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "InvokeV3::resource_bounds",
            },
        )?)?;

        let tip = Tip(value.tip);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "InvokeV3::signature",
                })?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        let nonce = Nonce(
            value
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "InvokeV3::nonce",
                })?
                .try_into()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField { field_description: "InvokeV3::sender" })?
            .try_into()?;

        let calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let calldata = Calldata(calldata.into());

        let nonce_data_availability_mode =
            enum_int_to_volition_domain(value.nonce_data_availability_mode)?;

        let fee_data_availability_mode =
            enum_int_to_volition_domain(value.fee_data_availability_mode)?;

        let paymaster_data = PaymasterData(
            value.paymaster_data.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?,
        );

        let account_deployment_data = AccountDeploymentData(
            value
                .account_deployment_data
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

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

impl From<InvokeTransactionV3> for protobuf::transaction::InvokeV3 {
    fn from(value: InvokeTransactionV3) -> Self {
        Self {
            resource_bounds: Some(protobuf::ResourceBounds::from(value.resource_bounds)),
            tip: value.tip.0,
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|signature| signature.into()).collect(),
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

impl TryFrom<protobuf::transaction::DeclareV0> for DeclareTransactionV0V1 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeclareV0) -> Result<Self, Self::Error> {
        let max_fee_felt =
            Felt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV0::max_fee",
            })?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV0::signature",
                })?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        // V0 transactions don't have a nonce, but the StarkNet API adds one to them
        let nonce = Nonce::default();

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV0::class_hash",
                })?
                .try_into()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV0::sender",
            })?
            .try_into()?;

        Ok(Self { max_fee, signature, nonce, class_hash, sender_address })
    }
}

impl From<DeclareTransactionV0V1> for protobuf::transaction::DeclareV0 {
    fn from(value: DeclareTransactionV0V1) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|stark_felt| stark_felt.into()).collect(),
            }),
            sender: Some(value.sender_address.into()),
            class_hash: Some(value.class_hash.0.into()),
        }
    }
}

impl TryFrom<protobuf::transaction::DeclareV1> for DeclareTransactionV0V1 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeclareV1) -> Result<Self, Self::Error> {
        let max_fee_felt =
            Felt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV1::max_fee",
            })?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV1::signature",
                })?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        let nonce = Nonce(
            value
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV1::nonce",
                })?
                .try_into()?,
        );

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV1::class_hash",
                })?
                .try_into()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV1::sender",
            })?
            .try_into()?;

        Ok(Self { max_fee, signature, nonce, class_hash, sender_address })
    }
}

impl From<DeclareTransactionV0V1> for protobuf::transaction::DeclareV1 {
    fn from(value: DeclareTransactionV0V1) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|stark_felt| stark_felt.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            class_hash: Some(value.class_hash.0.into()),
            sender: Some(value.sender_address.into()),
        }
    }
}

impl TryFrom<protobuf::transaction::DeclareV2> for DeclareTransactionV2 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeclareV2) -> Result<Self, Self::Error> {
        let max_fee_felt =
            Felt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV2::max_fee",
            })?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV2::signature",
                })?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        let nonce = Nonce(
            value
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV2::nonce",
                })?
                .try_into()?,
        );

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV2::class_hash",
                })?
                .try_into()?,
        );

        let compiled_class_hash = CompiledClassHash(
            value
                .compiled_class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV2::compiled_class_hash",
                })?
                .try_into()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV2::sender",
            })?
            .try_into()?;

        Ok(Self { max_fee, signature, nonce, class_hash, compiled_class_hash, sender_address })
    }
}

impl From<DeclareTransactionV2> for protobuf::transaction::DeclareV2 {
    fn from(value: DeclareTransactionV2) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|signature| signature.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            class_hash: Some(value.class_hash.0.into()),
            compiled_class_hash: Some(value.compiled_class_hash.0.into()),
            sender: Some(value.sender_address.into()),
        }
    }
}

impl TryFrom<protobuf::transaction::DeclareV3> for DeclareTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeclareV3) -> Result<Self, Self::Error> {
        let resource_bounds = ValidResourceBounds::try_from(value.resource_bounds.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeclareV3::resource_bounds",
            },
        )?)?;

        let tip = Tip(value.tip);

        let signature = TransactionSignature(
            value
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
            value
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV3::nonce",
                })?
                .try_into()?,
        );

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV3::class_hash",
                })?
                .try_into()?,
        );

        let compiled_class_hash = CompiledClassHash(
            value
                .compiled_class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV3::compiled_class_hash",
                })?
                .try_into()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV3::sender",
            })?
            .try_into()?;

        let nonce_data_availability_mode =
            enum_int_to_volition_domain(value.nonce_data_availability_mode)?;

        let fee_data_availability_mode =
            enum_int_to_volition_domain(value.fee_data_availability_mode)?;

        let paymaster_data = PaymasterData(
            value.paymaster_data.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?,
        );

        let account_deployment_data = AccountDeploymentData(
            value
                .account_deployment_data
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        Ok(Self {
            resource_bounds,
            tip,
            signature,
            nonce,
            class_hash,
            compiled_class_hash,
            sender_address,
            nonce_data_availability_mode,
            fee_data_availability_mode,
            paymaster_data,
            account_deployment_data,
        })
    }
}

impl From<DeclareTransactionV3> for protobuf::transaction::DeclareV3 {
    fn from(value: DeclareTransactionV3) -> Self {
        Self {
            resource_bounds: Some(protobuf::ResourceBounds::from(value.resource_bounds)),
            tip: value.tip.0,
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|signature| signature.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            class_hash: Some(value.class_hash.0.into()),
            compiled_class_hash: Some(value.compiled_class_hash.0.into()),
            sender: Some(value.sender_address.into()),
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

impl TryFrom<protobuf::transaction::Deploy> for DeployTransaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::Deploy) -> Result<Self, Self::Error> {
        let version = TransactionVersion(Felt::from(value.version));

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "Deploy::class_hash",
                })?
                .try_into()?,
        );

        let contract_address_salt = ContractAddressSalt(
            value
                .address_salt
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "Deploy::address_salt",
                })?
                .try_into()?,
        );

        let constructor_calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let constructor_calldata = Calldata(constructor_calldata.into());

        Ok(Self { version, class_hash, contract_address_salt, constructor_calldata })
    }
}

impl From<DeployTransaction> for protobuf::transaction::Deploy {
    fn from(value: DeployTransaction) -> Self {
        Self {
            version: try_from_starkfelt_to_u32(value.version.0).unwrap_or_default(),
            class_hash: Some(value.class_hash.0.into()),
            address_salt: Some(value.contract_address_salt.0.into()),
            calldata: value
                .constructor_calldata
                .0
                .iter()
                .map(|calldata| (*calldata).into())
                .collect(),
        }
    }
}

impl TryFrom<protobuf::transaction::L1HandlerV0> for L1HandlerTransaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::L1HandlerV0) -> Result<Self, Self::Error> {
        let version = L1HandlerTransaction::VERSION;

        let nonce = Nonce(
            value
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "L1HandlerV0::nonce",
                })?
                .try_into()?,
        );

        let contract_address = value
            .address
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "L1HandlerV0::address",
            })?
            .try_into()?;

        let entry_point_selector_felt = Felt::try_from(value.entry_point_selector.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "L1HandlerV0::entry_point_selector",
            },
        )?)?;
        let entry_point_selector = EntryPointSelector(entry_point_selector_felt);

        let calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let calldata = Calldata(calldata.into());

        Ok(Self { version, nonce, contract_address, entry_point_selector, calldata })
    }
}

impl From<L1HandlerTransaction> for protobuf::transaction::L1HandlerV0 {
    fn from(value: L1HandlerTransaction) -> Self {
        Self {
            nonce: Some(value.nonce.0.into()),
            address: Some(value.contract_address.into()),
            entry_point_selector: Some(value.entry_point_selector.0.into()),
            calldata: value.calldata.0.iter().map(|calldata| (*calldata).into()).collect(),
        }
    }
}
