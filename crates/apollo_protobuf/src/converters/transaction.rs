#[cfg(test)]
#[path = "transaction_test.rs"]
mod transaction_test;
use std::convert::{TryFrom, TryInto};

use prost::Message;
use starknet_api::block::GasPrice;
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::core::{ClassHash, CompiledClassHash, EntryPointSelector, Nonce};
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    Fee,
    PaymasterData,
    ProofFacts,
    ResourceBounds,
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
    FullTransaction,
    InvokeTransaction,
    InvokeTransactionV0,
    InvokeTransactionV1,
    InvokeTransactionV3,
    L1HandlerTransaction,
    Transaction,
    TransactionHash,
    TransactionOutput,
    TransactionVersion,
};
use starknet_types_core::felt::Felt;

use super::common::{
    enum_int_to_volition_domain,
    missing,
    try_from_starkfelt_to_u128,
    try_from_starkfelt_to_u32,
    volition_domain_to_enum_int,
};
use super::ProtobufConversionError;
use crate::sync::{DataOrFin, Query, TransactionQuery};
use crate::transaction::DeclareTransactionV3Common;
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

impl TryFrom<protobuf::TransactionsResponse> for DataOrFin<FullTransaction> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::TransactionsResponse) -> Result<Self, Self::Error> {
        let Some(transaction_message) = value.transaction_message else {
            return Err(missing("TransactionsResponse::transaction_message"));
        };

        match transaction_message {
            protobuf::transactions_response::TransactionMessage::TransactionWithReceipt(
                tx_with_receipt,
            ) => {
                let result: FullTransaction = tx_with_receipt.try_into()?;
                Ok(DataOrFin(Some(result)))
            }
            protobuf::transactions_response::TransactionMessage::Fin(_) => Ok(DataOrFin(None)),
        }
    }
}
impl From<DataOrFin<FullTransaction>> for protobuf::TransactionsResponse {
    fn from(value: DataOrFin<FullTransaction>) -> Self {
        match value.0 {
            Some(FullTransaction { transaction, transaction_output, transaction_hash }) => {
                protobuf::TransactionsResponse {
                    transaction_message: Some(
                        protobuf::transactions_response::TransactionMessage::TransactionWithReceipt(
                            protobuf::TransactionWithReceipt::from(FullTransaction {
                                transaction,
                                transaction_output,
                                transaction_hash,
                            }),
                        ),
                    ),
                }
            }
            None => protobuf::TransactionsResponse {
                transaction_message: Some(
                    protobuf::transactions_response::TransactionMessage::Fin(protobuf::Fin {}),
                ),
            },
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(DataOrFin<FullTransaction>, protobuf::TransactionsResponse);

impl TryFrom<protobuf::TransactionWithReceipt> for FullTransaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::TransactionWithReceipt) -> Result<Self, Self::Error> {
        let (transaction, transaction_hash) = <(Transaction, TransactionHash)>::try_from(
            value.transaction.ok_or(missing("TransactionWithReceipt::transaction"))?,
        )?;

        let transaction_output = TransactionOutput::try_from(
            value.receipt.ok_or(missing("TransactionWithReceipt::output"))?,
        )?;
        Ok(FullTransaction { transaction, transaction_output, transaction_hash })
    }
}

impl From<FullTransaction> for protobuf::TransactionWithReceipt {
    fn from(value: FullTransaction) -> Self {
        let FullTransaction { transaction, transaction_output, transaction_hash } = value;
        let transaction = (transaction, transaction_hash).into();
        let mut receipt = transaction_output.into();
        set_price_unit_based_on_transaction(&mut receipt, &transaction);
        Self { transaction: Some(transaction), receipt: Some(receipt) }
    }
}

// Used when converting a protobuf::TransactionWithReceipt to a FullTransaction.
impl TryFrom<protobuf::TransactionInBlock> for (Transaction, TransactionHash) {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::TransactionInBlock) -> Result<Self, Self::Error> {
        let tx_hash = value
            .transaction_hash
            .clone()
            .ok_or(missing("Transaction::transaction_hash"))?
            .try_into()
            .map(TransactionHash)?;
        let txn = value.txn.ok_or(missing("Transaction::txn"))?;
        let transaction: Transaction = match txn {
            protobuf::transaction_in_block::Txn::DeclareV0(declare_v0) => Transaction::Declare(
                DeclareTransaction::V0(DeclareTransactionV0V1::try_from(declare_v0)?),
            ),
            protobuf::transaction_in_block::Txn::DeclareV1(declare_v1) => Transaction::Declare(
                DeclareTransaction::V1(DeclareTransactionV0V1::try_from(declare_v1)?),
            ),
            protobuf::transaction_in_block::Txn::DeclareV2(declare_v2) => Transaction::Declare(
                DeclareTransaction::V2(DeclareTransactionV2::try_from(declare_v2)?),
            ),
            protobuf::transaction_in_block::Txn::DeclareV3(declare_v3) => Transaction::Declare(
                DeclareTransaction::V3(DeclareTransactionV3::try_from(declare_v3)?),
            ),
            protobuf::transaction_in_block::Txn::Deploy(deploy) => {
                Transaction::Deploy(DeployTransaction::try_from(deploy)?)
            }
            protobuf::transaction_in_block::Txn::DeployAccountV1(deploy_account_v1) => {
                Transaction::DeployAccount(DeployAccountTransaction::V1(
                    DeployAccountTransactionV1::try_from(deploy_account_v1)?,
                ))
            }
            protobuf::transaction_in_block::Txn::DeployAccountV3(deploy_account_v3) => {
                Transaction::DeployAccount(DeployAccountTransaction::V3(
                    DeployAccountTransactionV3::try_from(deploy_account_v3)?,
                ))
            }
            protobuf::transaction_in_block::Txn::InvokeV0(invoke_v0) => Transaction::Invoke(
                InvokeTransaction::V0(InvokeTransactionV0::try_from(invoke_v0)?),
            ),
            protobuf::transaction_in_block::Txn::InvokeV1(invoke_v1) => Transaction::Invoke(
                InvokeTransaction::V1(InvokeTransactionV1::try_from(invoke_v1)?),
            ),
            protobuf::transaction_in_block::Txn::InvokeV3(invoke_v3) => Transaction::Invoke(
                InvokeTransaction::V3(InvokeTransactionV3::try_from(invoke_v3)?),
            ),
            protobuf::transaction_in_block::Txn::L1Handler(l1_handler) => {
                Transaction::L1Handler(L1HandlerTransaction::try_from(l1_handler)?)
            }
        };
        Ok((transaction, tx_hash))
    }
}

impl From<(Transaction, TransactionHash)> for protobuf::TransactionInBlock {
    fn from(value: (Transaction, TransactionHash)) -> Self {
        let tx_hash = Some(value.1.0.into());
        match value.0 {
            Transaction::Declare(DeclareTransaction::V0(declare_v0)) => {
                protobuf::TransactionInBlock {
                    txn: Some(protobuf::transaction_in_block::Txn::DeclareV0(declare_v0.into())),
                    transaction_hash: tx_hash,
                }
            }
            Transaction::Declare(DeclareTransaction::V1(declare_v1)) => {
                protobuf::TransactionInBlock {
                    txn: Some(protobuf::transaction_in_block::Txn::DeclareV1(declare_v1.into())),
                    transaction_hash: tx_hash,
                }
            }
            Transaction::Declare(DeclareTransaction::V2(declare_v2)) => {
                protobuf::TransactionInBlock {
                    txn: Some(protobuf::transaction_in_block::Txn::DeclareV2(declare_v2.into())),
                    transaction_hash: tx_hash,
                }
            }
            Transaction::Declare(DeclareTransaction::V3(declare_v3)) => {
                protobuf::TransactionInBlock {
                    txn: Some(protobuf::transaction_in_block::Txn::DeclareV3(declare_v3.into())),
                    transaction_hash: tx_hash,
                }
            }
            Transaction::Deploy(deploy) => protobuf::TransactionInBlock {
                txn: Some(protobuf::transaction_in_block::Txn::Deploy(deploy.into())),
                transaction_hash: tx_hash,
            },
            Transaction::DeployAccount(deploy_account) => match deploy_account {
                DeployAccountTransaction::V1(deploy_account_v1) => protobuf::TransactionInBlock {
                    txn: Some(protobuf::transaction_in_block::Txn::DeployAccountV1(
                        deploy_account_v1.into(),
                    )),
                    transaction_hash: tx_hash,
                },
                DeployAccountTransaction::V3(deploy_account_v3) => protobuf::TransactionInBlock {
                    txn: Some(protobuf::transaction_in_block::Txn::DeployAccountV3(
                        deploy_account_v3.into(),
                    )),
                    transaction_hash: tx_hash,
                },
            },
            Transaction::Invoke(invoke) => match invoke {
                InvokeTransaction::V0(invoke_v0) => protobuf::TransactionInBlock {
                    txn: Some(protobuf::transaction_in_block::Txn::InvokeV0(invoke_v0.into())),
                    transaction_hash: tx_hash,
                },
                InvokeTransaction::V1(invoke_v1) => protobuf::TransactionInBlock {
                    txn: Some(protobuf::transaction_in_block::Txn::InvokeV1(invoke_v1.into())),
                    transaction_hash: tx_hash,
                },
                InvokeTransaction::V3(invoke_v3) => protobuf::TransactionInBlock {
                    txn: Some(protobuf::transaction_in_block::Txn::InvokeV3(invoke_v3.into())),
                    transaction_hash: tx_hash,
                },
            },
            Transaction::L1Handler(l1_handler) => protobuf::TransactionInBlock {
                txn: Some(protobuf::transaction_in_block::Txn::L1Handler(l1_handler.into())),
                transaction_hash: tx_hash,
            },
        }
    }
}

impl TryFrom<protobuf::transaction_in_block::DeployAccountV1> for DeployAccountTransactionV1 {
    type Error = ProtobufConversionError;
    fn try_from(
        value: protobuf::transaction_in_block::DeployAccountV1,
    ) -> Result<Self, Self::Error> {
        let max_fee_felt =
            Felt::try_from(value.max_fee.ok_or(missing("DeployAccountV1::max_fee"))?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(missing("DeployAccountV1::signature"))?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        );

        let nonce = Nonce(value.nonce.ok_or(missing("DeployAccountV1::nonce"))?.try_into()?);

        let class_hash =
            ClassHash(value.class_hash.ok_or(missing("DeployAccountV1::class_hash"))?.try_into()?);

        let contract_address_salt = ContractAddressSalt(
            value.address_salt.ok_or(missing("DeployAccountV1::address_salt"))?.try_into()?,
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

impl From<DeployAccountTransactionV1> for protobuf::transaction_in_block::DeployAccountV1 {
    fn from(value: DeployAccountTransactionV1) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.iter().map(|stark_felt| (*stark_felt).into()).collect(),
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

impl TryFrom<protobuf::DeployAccountV3> for DeployAccountTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeployAccountV3) -> Result<Self, Self::Error> {
        let resource_bounds = ValidResourceBounds::try_from(
            value.resource_bounds.ok_or(missing("DeployAccountV3::resource_bounds"))?,
        )?;

        let tip = Tip(value.tip);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(missing("DeployAccountV3::signature"))?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        );

        let nonce = Nonce(value.nonce.ok_or(missing("DeployAccountV3::nonce"))?.try_into()?);

        let class_hash =
            ClassHash(value.class_hash.ok_or(missing("DeployAccountV3::class_hash"))?.try_into()?);

        let contract_address_salt = ContractAddressSalt(
            value.address_salt.ok_or(missing("DeployAccountV3::address_salt"))?.try_into()?,
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

impl From<DeployAccountTransactionV3> for protobuf::DeployAccountV3 {
    fn from(value: DeployAccountTransactionV3) -> Self {
        Self {
            resource_bounds: Some(protobuf::ResourceBounds::from(value.resource_bounds)),
            tip: value.tip.0,
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.iter().map(|stark_felt| (*stark_felt).into()).collect(),
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

impl TryFrom<protobuf::ResourceBounds> for ValidResourceBounds {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ResourceBounds) -> Result<Self, Self::Error> {
        let Some(l1_gas) = value.l1_gas else {
            return Err(missing("ResourceBounds::l1_gas"));
        };
        let Some(l2_gas) = value.l2_gas else {
            return Err(missing("ResourceBounds::l2_gas"));
        };
        // TODO(Shahak): Assert data gas is not none once we remove support for 0.13.2.
        let l1_data_gas = value.l1_data_gas.unwrap_or_default();
        let l1_gas: ResourceBounds = l1_gas.try_into()?;
        let l2_gas: ResourceBounds = l2_gas.try_into()?;
        let l1_data_gas: ResourceBounds = l1_data_gas.try_into()?;
        Ok(if l1_data_gas.is_zero() && l2_gas.is_zero() {
            ValidResourceBounds::L1Gas(l1_gas)
        } else {
            ValidResourceBounds::AllResources(AllResourceBounds { l1_gas, l2_gas, l1_data_gas })
        })
    }
}

impl TryFrom<protobuf::ResourceLimits> for ResourceBounds {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ResourceLimits) -> Result<Self, Self::Error> {
        let max_amount = value.max_amount;
        let max_price_per_unit_felt = Felt::try_from(
            value
                .max_price_per_unit
                .ok_or(missing("ResourceBounds::ResourceLimits::max_price_per_unit"))?,
        )?;
        let max_price_per_unit =
            try_from_starkfelt_to_u128(max_price_per_unit_felt).map_err(|_| {
                ProtobufConversionError::OutOfRangeValue {
                    type_description: "u128",
                    value_as_str: format!("{max_price_per_unit_felt:?}"),
                }
            })?;
        Ok(ResourceBounds {
            max_amount: GasAmount(max_amount),
            max_price_per_unit: GasPrice(max_price_per_unit),
        })
    }
}

impl From<ResourceBounds> for protobuf::ResourceLimits {
    fn from(value: ResourceBounds) -> Self {
        protobuf::ResourceLimits {
            max_amount: value.max_amount.0,
            max_price_per_unit: Some(Felt::from(value.max_price_per_unit.0).into()),
        }
    }
}

impl From<ValidResourceBounds> for protobuf::ResourceBounds {
    fn from(value: ValidResourceBounds) -> Self {
        match value {
            ValidResourceBounds::L1Gas(l1_gas) => protobuf::ResourceBounds {
                l1_gas: Some(l1_gas.into()),
                l2_gas: Some(value.get_l2_bounds().into()),
                l1_data_gas: Some(ResourceBounds::default().into()),
            },
            ValidResourceBounds::AllResources(AllResourceBounds {
                l1_gas,
                l2_gas,
                l1_data_gas,
            }) => protobuf::ResourceBounds {
                l1_gas: Some(l1_gas.into()),
                l2_gas: Some(l2_gas.into()),
                l1_data_gas: Some(l1_data_gas.into()),
            },
        }
    }
}

impl TryFrom<protobuf::transaction_in_block::InvokeV0> for InvokeTransactionV0 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction_in_block::InvokeV0) -> Result<Self, Self::Error> {
        let max_fee_felt = Felt::try_from(value.max_fee.ok_or(missing("InvokeV0::max_fee"))?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(missing("InvokeV0::signature"))?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        );

        let contract_address = value.address.ok_or(missing("InvokeV0::address"))?.try_into()?;

        let entry_point_selector_felt = Felt::try_from(
            value.entry_point_selector.ok_or(missing("InvokeV0::entry_point_selector"))?,
        )?;
        let entry_point_selector = EntryPointSelector(entry_point_selector_felt);

        let calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let calldata = Calldata(calldata.into());

        Ok(Self { max_fee, signature, contract_address, entry_point_selector, calldata })
    }
}

impl From<InvokeTransactionV0> for protobuf::transaction_in_block::InvokeV0 {
    fn from(value: InvokeTransactionV0) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.iter().map(|stark_felt| (*stark_felt).into()).collect(),
            }),
            address: Some(value.contract_address.into()),
            entry_point_selector: Some(value.entry_point_selector.0.into()),
            calldata: value.calldata.0.iter().map(|calldata| (*calldata).into()).collect(),
        }
    }
}

impl TryFrom<protobuf::transaction_in_block::InvokeV1> for InvokeTransactionV1 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction_in_block::InvokeV1) -> Result<Self, Self::Error> {
        let max_fee_felt = Felt::try_from(value.max_fee.ok_or(missing("InvokeV1::max_fee"))?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(missing("InvokeV1::signature"))?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        );

        let sender_address = value.sender.ok_or(missing("InvokeV1::sender"))?.try_into()?;

        let nonce = Nonce(value.nonce.ok_or(missing("InvokeV1::nonce"))?.try_into()?);

        let calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let calldata = Calldata(calldata.into());

        Ok(Self { max_fee, signature, nonce, sender_address, calldata })
    }
}

impl From<InvokeTransactionV1> for protobuf::transaction_in_block::InvokeV1 {
    fn from(value: InvokeTransactionV1) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.iter().map(|signature| (*signature).into()).collect(),
            }),
            sender: Some(value.sender_address.into()),
            nonce: Some(value.nonce.0.into()),
            calldata: value.calldata.0.iter().map(|calldata| (*calldata).into()).collect(),
        }
    }
}

impl TryFrom<protobuf::InvokeV3> for InvokeTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::InvokeV3) -> Result<Self, Self::Error> {
        let resource_bounds = ValidResourceBounds::try_from(
            value.resource_bounds.ok_or(missing("InvokeV3::resource_bounds"))?,
        )?;

        let tip = Tip(value.tip);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(missing("InvokeV3::signature"))?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        );

        let nonce = Nonce(value.nonce.ok_or(missing("InvokeV3::nonce"))?.try_into()?);

        let sender_address = value.sender.ok_or(missing("InvokeV3::sender"))?.try_into()?;

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
            // TODO(AvivG): Get proof_facts from P2P protocol, until then, lost during protobuf
            // serialization/deserialization.
            proof_facts: ProofFacts::default(),
        })
    }
}

impl From<InvokeTransactionV3> for protobuf::InvokeV3 {
    fn from(value: InvokeTransactionV3) -> Self {
        Self {
            resource_bounds: Some(protobuf::ResourceBounds::from(value.resource_bounds)),
            tip: value.tip.0,
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.iter().map(|signature| (*signature).into()).collect(),
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

impl TryFrom<protobuf::transaction_in_block::DeclareV0WithoutClass> for DeclareTransactionV0V1 {
    type Error = ProtobufConversionError;
    fn try_from(
        value: protobuf::transaction_in_block::DeclareV0WithoutClass,
    ) -> Result<Self, Self::Error> {
        let max_fee_felt = Felt::try_from(value.max_fee.ok_or(missing("DeclareV0::max_fee"))?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(missing("DeclareV0::signature"))?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        );

        // V0 transactions don't have a nonce, but the StarkNet API adds one to them
        let nonce = Nonce::default();

        let class_hash =
            ClassHash(value.class_hash.ok_or(missing("DeclareV0::class_hash"))?.try_into()?);

        let sender_address = value.sender.ok_or(missing("DeclareV0::sender"))?.try_into()?;

        Ok(Self { max_fee, signature, nonce, class_hash, sender_address })
    }
}

impl From<DeclareTransactionV0V1> for protobuf::transaction_in_block::DeclareV0WithoutClass {
    fn from(value: DeclareTransactionV0V1) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.iter().map(|stark_felt| (*stark_felt).into()).collect(),
            }),
            sender: Some(value.sender_address.into()),
            class_hash: Some(value.class_hash.0.into()),
        }
    }
}

impl TryFrom<protobuf::transaction_in_block::DeclareV1WithoutClass> for DeclareTransactionV0V1 {
    type Error = ProtobufConversionError;
    fn try_from(
        value: protobuf::transaction_in_block::DeclareV1WithoutClass,
    ) -> Result<Self, Self::Error> {
        let max_fee_felt = Felt::try_from(value.max_fee.ok_or(missing("DeclareV1::max_fee"))?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(missing("DeclareV1::signature"))?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        );

        let nonce = Nonce(value.nonce.ok_or(missing("DeclareV1::nonce"))?.try_into()?);

        let class_hash =
            ClassHash(value.class_hash.ok_or(missing("DeclareV1::class_hash"))?.try_into()?);

        let sender_address = value.sender.ok_or(missing("DeclareV1::sender"))?.try_into()?;

        Ok(Self { max_fee, signature, nonce, class_hash, sender_address })
    }
}

impl From<DeclareTransactionV0V1> for protobuf::transaction_in_block::DeclareV1WithoutClass {
    fn from(value: DeclareTransactionV0V1) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.iter().map(|stark_felt| (*stark_felt).into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            class_hash: Some(value.class_hash.0.into()),
            sender: Some(value.sender_address.into()),
        }
    }
}

impl TryFrom<protobuf::transaction_in_block::DeclareV2WithoutClass> for DeclareTransactionV2 {
    type Error = ProtobufConversionError;
    fn try_from(
        value: protobuf::transaction_in_block::DeclareV2WithoutClass,
    ) -> Result<Self, Self::Error> {
        let max_fee_felt = Felt::try_from(value.max_fee.ok_or(missing("DeclareV2::max_fee"))?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(missing("DeclareV2::signature"))?
                .parts
                .into_iter()
                .map(Felt::try_from)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        );

        let nonce = Nonce(value.nonce.ok_or(missing("DeclareV2::nonce"))?.try_into()?);

        let class_hash =
            ClassHash(value.class_hash.ok_or(missing("DeclareV2::class_hash"))?.try_into()?);

        let compiled_class_hash = CompiledClassHash(
            value
                .compiled_class_hash
                .ok_or(missing("DeclareV2::compiled_class_hash"))?
                .try_into()?,
        );

        let sender_address = value.sender.ok_or(missing("DeclareV2::sender"))?.try_into()?;

        Ok(Self { max_fee, signature, nonce, class_hash, compiled_class_hash, sender_address })
    }
}

impl From<DeclareTransactionV2> for protobuf::transaction_in_block::DeclareV2WithoutClass {
    fn from(value: DeclareTransactionV2) -> Self {
        Self {
            max_fee: Some(Felt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.iter().map(|signature| (*signature).into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            class_hash: Some(value.class_hash.0.into()),
            compiled_class_hash: Some(value.compiled_class_hash.0.into()),
            sender: Some(value.sender_address.into()),
        }
    }
}

impl TryFrom<protobuf::transaction_in_block::DeclareV3WithoutClass>
    for (DeclareTransactionV3Common, ClassHash)
{
    type Error = ProtobufConversionError;
    fn try_from(
        value: protobuf::transaction_in_block::DeclareV3WithoutClass,
    ) -> Result<Self, Self::Error> {
        let common = DeclareTransactionV3Common::try_from(
            value.common.ok_or(missing("DeclareV3WithoutClass::common"))?,
        )?;
        let class_hash = ClassHash(
            value.class_hash.ok_or(missing("DeclareV3WithoutClass::class_hash"))?.try_into()?,
        );
        Ok((common, class_hash))
    }
}

impl From<(DeclareTransactionV3Common, ClassHash)>
    for protobuf::transaction_in_block::DeclareV3WithoutClass
{
    fn from(value: (DeclareTransactionV3Common, ClassHash)) -> Self {
        Self { common: Some(value.0.into()), class_hash: Some(value.1.0.into()) }
    }
}

impl TryFrom<protobuf::transaction_in_block::DeclareV3WithoutClass> for DeclareTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(
        value: protobuf::transaction_in_block::DeclareV3WithoutClass,
    ) -> Result<Self, Self::Error> {
        let (common, class_hash) = value.try_into()?;

        Ok(Self {
            resource_bounds: common.resource_bounds,
            tip: common.tip,
            signature: common.signature,
            nonce: common.nonce,
            class_hash,
            compiled_class_hash: common.compiled_class_hash,
            sender_address: common.sender_address,
            nonce_data_availability_mode: common.nonce_data_availability_mode,
            fee_data_availability_mode: common.fee_data_availability_mode,
            paymaster_data: common.paymaster_data,
            account_deployment_data: common.account_deployment_data,
        })
    }
}

impl From<DeclareTransactionV3> for protobuf::transaction_in_block::DeclareV3WithoutClass {
    fn from(value: DeclareTransactionV3) -> Self {
        let common = DeclareTransactionV3Common {
            resource_bounds: value.resource_bounds,
            tip: value.tip,
            signature: value.signature,
            nonce: value.nonce,
            compiled_class_hash: value.compiled_class_hash,
            sender_address: value.sender_address,
            nonce_data_availability_mode: value.nonce_data_availability_mode,
            fee_data_availability_mode: value.fee_data_availability_mode,
            paymaster_data: value.paymaster_data,
            account_deployment_data: value.account_deployment_data,
        };
        let class_hash = value.class_hash;
        Self { common: Some(common.into()), class_hash: Some(class_hash.0.into()) }
    }
}

impl TryFrom<protobuf::transaction_in_block::Deploy> for DeployTransaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction_in_block::Deploy) -> Result<Self, Self::Error> {
        let version = TransactionVersion(Felt::from(value.version));

        let class_hash =
            ClassHash(value.class_hash.ok_or(missing("Deploy::class_hash"))?.try_into()?);

        let contract_address_salt = ContractAddressSalt(
            value.address_salt.ok_or(missing("Deploy::address_salt"))?.try_into()?,
        );

        let constructor_calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let constructor_calldata = Calldata(constructor_calldata.into());

        Ok(Self { version, class_hash, contract_address_salt, constructor_calldata })
    }
}

impl From<DeployTransaction> for protobuf::transaction_in_block::Deploy {
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

impl TryFrom<protobuf::L1HandlerV0> for L1HandlerTransaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::L1HandlerV0) -> Result<Self, Self::Error> {
        let version = L1HandlerTransaction::VERSION;

        let nonce = Nonce(value.nonce.ok_or(missing("L1HandlerV0::nonce"))?.try_into()?);

        let contract_address = value.address.ok_or(missing("L1HandlerV0::address"))?.try_into()?;

        let entry_point_selector_felt = Felt::try_from(
            value.entry_point_selector.ok_or(missing("L1HandlerV0::entry_point_selector"))?,
        )?;
        let entry_point_selector = EntryPointSelector(entry_point_selector_felt);

        let calldata =
            value.calldata.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let calldata = Calldata(calldata.into());

        Ok(Self { version, nonce, contract_address, entry_point_selector, calldata })
    }
}

impl From<L1HandlerTransaction> for protobuf::L1HandlerV0 {
    fn from(value: L1HandlerTransaction) -> Self {
        Self {
            nonce: Some(value.nonce.0.into()),
            address: Some(value.contract_address.into()),
            entry_point_selector: Some(value.entry_point_selector.0.into()),
            calldata: value.calldata.0.iter().map(|calldata| (*calldata).into()).collect(),
        }
    }
}

impl From<ConsensusTransaction> for protobuf::ConsensusTransaction {
    fn from(value: ConsensusTransaction) -> Self {
        match value {
            ConsensusTransaction::RpcTransaction(RpcTransaction::Declare(
                RpcDeclareTransaction::V3(txn),
            )) => protobuf::ConsensusTransaction {
                txn: Some(protobuf::consensus_transaction::Txn::DeclareV3(txn.into())),
                transaction_hash: None,
            },
            ConsensusTransaction::RpcTransaction(RpcTransaction::DeployAccount(
                RpcDeployAccountTransaction::V3(txn),
            )) => protobuf::ConsensusTransaction {
                txn: Some(protobuf::consensus_transaction::Txn::DeployAccountV3(txn.into())),
                transaction_hash: None,
            },
            ConsensusTransaction::RpcTransaction(RpcTransaction::Invoke(
                RpcInvokeTransaction::V3(txn),
            )) => protobuf::ConsensusTransaction {
                txn: Some(protobuf::consensus_transaction::Txn::InvokeV3(txn.into())),
                transaction_hash: None,
            },
            ConsensusTransaction::L1Handler(txn) => protobuf::ConsensusTransaction {
                txn: Some(protobuf::consensus_transaction::Txn::L1Handler(txn.into())),
                transaction_hash: None,
            },
        }
    }
}

impl TryFrom<protobuf::ConsensusTransaction> for ConsensusTransaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ConsensusTransaction) -> Result<Self, Self::Error> {
        let txn = value.txn.ok_or(missing("ConsensusTransaction::txn"))?;
        let txn = match txn {
            protobuf::consensus_transaction::Txn::DeclareV3(txn) => {
                ConsensusTransaction::RpcTransaction(RpcTransaction::Declare(
                    RpcDeclareTransaction::V3(txn.try_into()?),
                ))
            }
            protobuf::consensus_transaction::Txn::DeployAccountV3(txn) => {
                ConsensusTransaction::RpcTransaction(RpcTransaction::DeployAccount(
                    RpcDeployAccountTransaction::V3(txn.try_into()?),
                ))
            }
            protobuf::consensus_transaction::Txn::InvokeV3(txn) => {
                ConsensusTransaction::RpcTransaction(RpcTransaction::Invoke(
                    RpcInvokeTransaction::V3(txn.try_into()?),
                ))
            }
            protobuf::consensus_transaction::Txn::L1Handler(txn) => {
                ConsensusTransaction::L1Handler(txn.try_into()?)
            }
        };
        Ok(txn)
    }
}

impl TryFrom<protobuf::TransactionsRequest> for Query {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::TransactionsRequest) -> Result<Self, Self::Error> {
        Ok(TransactionQuery::try_from(value)?.0)
    }
}

impl TryFrom<protobuf::TransactionsRequest> for TransactionQuery {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::TransactionsRequest) -> Result<Self, Self::Error> {
        Ok(TransactionQuery(
            value.iteration.ok_or(missing("TransactionsRequest::iteration"))?.try_into()?,
        ))
    }
}

impl From<Query> for protobuf::TransactionsRequest {
    fn from(value: Query) -> Self {
        protobuf::TransactionsRequest { iteration: Some(value.into()) }
    }
}

impl From<TransactionQuery> for protobuf::TransactionsRequest {
    fn from(value: TransactionQuery) -> Self {
        protobuf::TransactionsRequest { iteration: Some(value.0.into()) }
    }
}

auto_impl_into_and_try_from_vec_u8!(TransactionQuery, protobuf::TransactionsRequest);

pub fn set_price_unit_based_on_transaction(
    receipt: &mut protobuf::Receipt,
    transaction: &protobuf::TransactionInBlock,
) {
    let price_unit = match &transaction.txn {
        Some(protobuf::transaction_in_block::Txn::DeclareV1(_)) => protobuf::PriceUnit::Wei,
        Some(protobuf::transaction_in_block::Txn::DeclareV2(_)) => protobuf::PriceUnit::Wei,
        Some(protobuf::transaction_in_block::Txn::DeclareV3(_)) => protobuf::PriceUnit::Fri,
        Some(protobuf::transaction_in_block::Txn::Deploy(_)) => protobuf::PriceUnit::Wei,
        Some(protobuf::transaction_in_block::Txn::DeployAccountV1(_)) => protobuf::PriceUnit::Wei,
        Some(protobuf::transaction_in_block::Txn::DeployAccountV3(_)) => protobuf::PriceUnit::Fri,
        Some(protobuf::transaction_in_block::Txn::InvokeV1(_)) => protobuf::PriceUnit::Wei,
        Some(protobuf::transaction_in_block::Txn::InvokeV3(_)) => protobuf::PriceUnit::Fri,
        Some(protobuf::transaction_in_block::Txn::L1Handler(_)) => protobuf::PriceUnit::Wei,
        _ => return,
    };
    let Some(ref mut receipt_type) = receipt.r#type else {
        return;
    };

    let common = match receipt_type {
        protobuf::receipt::Type::Invoke(invoke) => invoke.common.as_mut(),
        protobuf::receipt::Type::L1Handler(l1_handler) => l1_handler.common.as_mut(),
        protobuf::receipt::Type::Declare(declare) => declare.common.as_mut(),
        protobuf::receipt::Type::DeprecatedDeploy(deploy) => deploy.common.as_mut(),
        protobuf::receipt::Type::DeployAccount(deploy_account) => deploy_account.common.as_mut(),
    };

    if let Some(common) = common {
        common.price_unit = price_unit.into();
    }
}
