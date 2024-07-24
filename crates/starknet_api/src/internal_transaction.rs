use crate::core::{ContractAddress, Nonce};
use crate::state::ContractClass;
use crate::transaction::{
    DeclareTransaction, DeployAccountTransaction, InvokeTransaction, Tip, TransactionHash,
};

/// Represents a paid Starknet transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InternalTransaction {
    Declare(InternalDeclareTransaction),
    DeployAccount(InternalDeployAccountTransaction),
    Invoke(InternalInvokeTransaction),
}

impl InternalTransaction {
    pub fn contract_address(&self) -> ContractAddress {
        match self {
            InternalTransaction::Declare(tx_data) => tx_data.tx.sender_address(),
            InternalTransaction::DeployAccount(tx_data) => tx_data.contract_address,
            InternalTransaction::Invoke(tx_data) => tx_data.tx.sender_address(),
        }
    }

    pub fn nonce(&self) -> Nonce {
        match self {
            InternalTransaction::Declare(tx_data) => tx_data.tx.nonce(),
            InternalTransaction::DeployAccount(tx_data) => tx_data.tx.nonce(),
            InternalTransaction::Invoke(tx_data) => tx_data.tx.nonce(),
        }
    }

    pub fn tx_hash(&self) -> TransactionHash {
        match self {
            InternalTransaction::Declare(tx_data) => tx_data.tx_hash,
            InternalTransaction::DeployAccount(tx_data) => tx_data.tx_hash,
            InternalTransaction::Invoke(tx_data) => tx_data.tx_hash,
        }
    }

    pub fn tip(&self) -> Option<Tip> {
        match self {
            InternalTransaction::Declare(declare_tx) => match &declare_tx.tx {
                DeclareTransaction::V3(tx_v3) => Some(tx_v3.tip),
                _ => None,
            },
            InternalTransaction::DeployAccount(deploy_account_tx) => match &deploy_account_tx.tx {
                DeployAccountTransaction::V3(tx_v3) => Some(tx_v3.tip),
                _ => None,
            },
            InternalTransaction::Invoke(invoke_tx) => match &invoke_tx.tx {
                InvokeTransaction::V3(tx_v3) => Some(tx_v3.tip),
                _ => None,
            },
        }
    }
}

// TODO(Mohammad): Add constructor for all the transaction's structs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalDeclareTransaction {
    pub tx: DeclareTransaction,
    pub tx_hash: TransactionHash,
    // Indicates the presence of the only_query bit in the version.
    pub only_query: bool,
    pub class_info: ClassInfo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalDeployAccountTransaction {
    pub tx: DeployAccountTransaction,
    pub tx_hash: TransactionHash,
    pub contract_address: ContractAddress,
    // Indicates the presence of the only_query bit in the version.
    pub only_query: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalInvokeTransaction {
    pub tx: InvokeTransaction,
    pub tx_hash: TransactionHash,
    // Indicates the presence of the only_query bit in the version.
    pub only_query: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassInfo {
    pub contract_class: ContractClass,
    pub sierra_program_length: usize,
    pub abi_length: usize,
}
