use serde::{Deserialize, Serialize};

use crate::contract_class::ClassInfo;
use crate::core::{calculate_contract_address, ChainId, ClassHash, ContractAddress, Nonce};
use crate::data_availability::DataAvailabilityMode;
use crate::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};
use crate::transaction::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    DeclareTransactionV3,
    DeployAccountTransactionV3,
    InvokeTransactionV3,
    PaymasterData,
    ResourceBoundsMapping,
    Tip,
    TransactionHash,
    TransactionHasher,
    TransactionSignature,
    TransactionVersion,
};
use crate::StarknetApiError;

macro_rules! implement_inner_tx_getter_calls {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            self.tx.$field().clone()
        })*
    };
}

macro_rules! implement_getter_calls {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            self.$field
        })*
    };
}

/// Represents a paid Starknet transaction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Transaction {
    Declare(DeclareTransaction),
    DeployAccount(DeployAccountTransaction),
    Invoke(InvokeTransaction),
}

impl Transaction {
    pub fn contract_address(&self) -> ContractAddress {
        match self {
            Transaction::Declare(tx_data) => tx_data.tx.sender_address(),
            Transaction::DeployAccount(tx_data) => tx_data.contract_address,
            Transaction::Invoke(tx_data) => tx_data.tx.sender_address(),
        }
    }

    pub fn nonce(&self) -> Nonce {
        match self {
            Transaction::Declare(tx_data) => tx_data.tx.nonce(),
            Transaction::DeployAccount(tx_data) => tx_data.tx.nonce(),
            Transaction::Invoke(tx_data) => tx_data.tx.nonce(),
        }
    }

    pub fn tx_hash(&self) -> TransactionHash {
        match self {
            Transaction::Declare(tx_data) => tx_data.tx_hash,
            Transaction::DeployAccount(tx_data) => tx_data.tx_hash,
            Transaction::Invoke(tx_data) => tx_data.tx_hash,
        }
    }

    // TODO(Mohammad): add a getter macro.
    pub fn tip(&self) -> Option<Tip> {
        match self {
            Transaction::Declare(declare_tx) => match &declare_tx.tx {
                crate::transaction::DeclareTransaction::V3(tx_v3) => Some(tx_v3.tip),
                _ => None,
            },
            Transaction::DeployAccount(deploy_account_tx) => match &deploy_account_tx.tx {
                crate::transaction::DeployAccountTransaction::V3(tx_v3) => Some(tx_v3.tip),
                _ => None,
            },
            Transaction::Invoke(invoke_tx) => match &invoke_tx.tx {
                crate::transaction::InvokeTransaction::V3(tx_v3) => Some(tx_v3.tip),
                _ => None,
            },
        }
    }

    pub fn resource_bounds(&self) -> Option<&ResourceBoundsMapping> {
        match self {
            Transaction::Declare(declare_tx) => match &declare_tx.tx {
                crate::transaction::DeclareTransaction::V3(tx_v3) => Some(&tx_v3.resource_bounds),
                _ => None,
            },
            Transaction::DeployAccount(deploy_account_tx) => match &deploy_account_tx.tx {
                crate::transaction::DeployAccountTransaction::V3(tx_v3) => {
                    Some(&tx_v3.resource_bounds)
                }
                _ => None,
            },
            Transaction::Invoke(invoke_tx) => match &invoke_tx.tx {
                crate::transaction::InvokeTransaction::V3(tx_v3) => Some(&tx_v3.resource_bounds),
                _ => None,
            },
        }
    }

    // TODO(Arni): Update the function to support all transaction types.
    pub fn new_from_rpc_tx(
        rpc_tx: RpcTransaction,
        tx_hash: TransactionHash,
        sender_address: ContractAddress,
    ) -> Transaction {
        Transaction::Invoke(crate::executable_transaction::InvokeTransaction {
            tx: crate::transaction::InvokeTransaction::V3(
                crate::transaction::InvokeTransactionV3 {
                    sender_address,
                    tip: *rpc_tx.tip(),
                    nonce: *rpc_tx.nonce(),
                    resource_bounds: rpc_tx.resource_bounds().clone().into(),
                    signature: TransactionSignature::default(),
                    calldata: Calldata::default(),
                    nonce_data_availability_mode: DataAvailabilityMode::L1,
                    fee_data_availability_mode: DataAvailabilityMode::L1,
                    paymaster_data: PaymasterData::default(),
                    account_deployment_data: AccountDeploymentData::default(),
                },
            ),
            tx_hash,
        })
    }

    /// Crates an executable transaction based on the given RPC declare transaction. For declare
    /// transaction class info is required.
    pub fn from_rpc_tx(
        rpc_tx: &RpcTransaction,
        class_info: Option<ClassInfo>,
        chain_id: &ChainId,
    ) -> Result<Transaction, StarknetApiError> {
        match rpc_tx {
            RpcTransaction::Declare(rpc_declare_tx) => {
                let class_info =
                    class_info.expect("Class info is required for declare transaction.");
                Ok(Transaction::Declare(DeclareTransaction::from_rpc_tx(
                    rpc_declare_tx,
                    class_info,
                    chain_id,
                )?))
            }
            RpcTransaction::DeployAccount(rpc_deploy_account_tx) => Ok(Transaction::DeployAccount(
                DeployAccountTransaction::from_rpc_tx(rpc_deploy_account_tx, chain_id)?,
            )),
            RpcTransaction::Invoke(rpc_invoke_tx) => {
                Ok(Transaction::Invoke(InvokeTransaction::from_rpc_tx(rpc_invoke_tx, chain_id)?))
            }
        }
    }
}

// TODO(Mohammad): Add constructor for all the transaction's structs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeclareTransaction {
    pub tx: crate::transaction::DeclareTransaction,
    pub tx_hash: TransactionHash,
    pub class_info: ClassInfo,
}

impl DeclareTransaction {
    /// Crates an executable declare transaction based on the given RPC declare transaction and the
    /// given class info. Note that relation of the given class info and transaction is unchecked.
    pub fn from_rpc_tx(
        rpc_tx: &RpcDeclareTransaction,
        class_info: ClassInfo,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        let RpcDeclareTransaction::V3(tx) = rpc_tx;
        let declare_tx = crate::transaction::DeclareTransaction::V3(DeclareTransactionV3 {
            class_hash: ClassHash::default(), /* FIXME(yael 15/4/24): call the starknet-api
                                               * function once ready */
            resource_bounds: tx.resource_bounds.clone().into(),
            tip: tx.tip,
            signature: tx.signature.clone(),
            nonce: tx.nonce,
            compiled_class_hash: tx.compiled_class_hash,
            sender_address: tx.sender_address,
            nonce_data_availability_mode: tx.nonce_data_availability_mode,
            fee_data_availability_mode: tx.fee_data_availability_mode,
            paymaster_data: tx.paymaster_data.clone(),
            account_deployment_data: tx.account_deployment_data.clone(),
        });
        let tx_hash = declare_tx.calculate_transaction_hash(chain_id, &declare_tx.version())?;
        Ok(Self { tx: declare_tx, tx_hash, class_info })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeployAccountTransaction {
    pub tx: crate::transaction::DeployAccountTransaction,
    pub tx_hash: TransactionHash,
    pub contract_address: ContractAddress,
}

impl DeployAccountTransaction {
    implement_inner_tx_getter_calls!(
        (class_hash, ClassHash),
        (constructor_calldata, Calldata),
        (contract_address_salt, ContractAddressSalt),
        (nonce, Nonce),
        (signature, TransactionSignature),
        (version, TransactionVersion)
    );
    implement_getter_calls!((tx_hash, TransactionHash), (contract_address, ContractAddress));

    pub fn tx(&self) -> &crate::transaction::DeployAccountTransaction {
        &self.tx
    }

    pub fn from_rpc_tx(
        rpc_tx: &RpcDeployAccountTransaction,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        let RpcDeployAccountTransaction::V3(tx) = rpc_tx;
        let deploy_account_tx =
            crate::transaction::DeployAccountTransaction::V3(DeployAccountTransactionV3 {
                resource_bounds: tx.resource_bounds.clone().into(),
                tip: tx.tip,
                signature: tx.signature.clone(),
                nonce: tx.nonce,
                class_hash: tx.class_hash,
                contract_address_salt: tx.contract_address_salt,
                constructor_calldata: tx.constructor_calldata.clone(),
                nonce_data_availability_mode: tx.nonce_data_availability_mode,
                fee_data_availability_mode: tx.fee_data_availability_mode,
                paymaster_data: tx.paymaster_data.clone(),
            });
        let contract_address = calculate_contract_address(
            deploy_account_tx.contract_address_salt(),
            deploy_account_tx.class_hash(),
            &deploy_account_tx.constructor_calldata(),
            ContractAddress::default(),
        )?;
        let tx_hash =
            deploy_account_tx.calculate_transaction_hash(chain_id, &deploy_account_tx.version())?;
        Ok(Self { tx: deploy_account_tx, tx_hash, contract_address })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InvokeTransaction {
    pub tx: crate::transaction::InvokeTransaction,
    pub tx_hash: TransactionHash,
}

impl InvokeTransaction {
    implement_inner_tx_getter_calls!(
        (calldata, Calldata),
        (nonce, Nonce),
        (signature, TransactionSignature),
        (sender_address, ContractAddress),
        (version, TransactionVersion)
    );
    implement_getter_calls!((tx_hash, TransactionHash));

    pub fn tx(&self) -> &crate::transaction::InvokeTransaction {
        &self.tx
    }

    pub fn from_rpc_tx(
        rpc_tx: &RpcInvokeTransaction,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        let RpcInvokeTransaction::V3(tx) = rpc_tx;
        let invoke_tx = crate::transaction::InvokeTransaction::V3(InvokeTransactionV3 {
            resource_bounds: tx.resource_bounds.clone().into(),
            tip: tx.tip,
            signature: tx.signature.clone(),
            nonce: tx.nonce,
            sender_address: tx.sender_address,
            calldata: tx.calldata.clone(),
            nonce_data_availability_mode: tx.nonce_data_availability_mode,
            fee_data_availability_mode: tx.fee_data_availability_mode,
            paymaster_data: tx.paymaster_data.clone(),
            account_deployment_data: tx.account_deployment_data.clone(),
        });
        let tx_hash = invoke_tx.calculate_transaction_hash(chain_id, &invoke_tx.version())?;
        Ok(Self { tx: invoke_tx, tx_hash })
    }
}
