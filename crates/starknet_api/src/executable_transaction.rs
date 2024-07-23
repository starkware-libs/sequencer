use crate::core::{ClassHash, ContractAddress, Nonce};
use crate::state::ContractClass;
use crate::transaction::{
    Calldata,
    ContractAddressSalt,
    Tip,
    TransactionHash,
    TransactionSignature,
    TransactionVersion,
};

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
#[derive(Clone, Debug, Eq, PartialEq)]
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
}

// TODO(Mohammad): Add constructor for all the transaction's structs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclareTransaction {
    pub tx: crate::transaction::DeclareTransaction,
    pub tx_hash: TransactionHash,
    pub class_info: ClassInfo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassInfo {
    // TODO: use compiled contract class.
    pub contract_class: ContractClass,
    pub sierra_program_length: usize,
    pub abi_length: usize,
}
