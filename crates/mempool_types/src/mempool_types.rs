use serde::{Deserialize, Serialize};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::executable_transaction::{InvokeTransaction, Transaction};
use starknet_api::transaction::{
    AccountDeploymentData,
    Calldata,
    PaymasterData,
    ResourceBoundsMapping,
    Tip,
    TransactionHash,
    TransactionSignature,
};

use crate::errors::MempoolError;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ThinTransaction {
    pub sender_address: ContractAddress,
    pub tx_hash: TransactionHash,
    pub tip: Tip,
    pub nonce: Nonce,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountState {
    pub nonce: Nonce,
    // TODO: add balance field when needed.
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Account {
    // TODO(Ayelet): Consider removing this field as it is duplicated in ThinTransaction.
    pub sender_address: ContractAddress,
    pub state: AccountState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MempoolInput {
    pub tx: Transaction,
    pub account: Account,
}

pub type MempoolResult<T> = Result<T, MempoolError>;

// TODO(Mohammad): Consider deleting these methods once `ThinTransaction` is deleted.
impl From<&Transaction> for ThinTransaction {
    fn from(tx: &Transaction) -> Self {
        ThinTransaction {
            sender_address: tx.contract_address(),
            tx_hash: tx.tx_hash(),
            tip: tx.tip().unwrap_or_default(),
            nonce: tx.nonce(),
        }
    }
}

impl From<&ThinTransaction> for Transaction {
    fn from(tx: &ThinTransaction) -> Self {
        Transaction::Invoke(InvokeTransaction {
            tx: starknet_api::transaction::InvokeTransaction::V3(
                starknet_api::transaction::InvokeTransactionV3 {
                    sender_address: tx.sender_address,
                    tip: tx.tip,
                    nonce: tx.nonce,
                    resource_bounds: ResourceBoundsMapping::default(),
                    signature: TransactionSignature::default(),
                    calldata: Calldata::default(),
                    nonce_data_availability_mode: DataAvailabilityMode::L1,
                    fee_data_availability_mode: DataAvailabilityMode::L2,
                    paymaster_data: PaymasterData::default(),
                    account_deployment_data: AccountDeploymentData::default(),
                },
            ),
            tx_hash: tx.tx_hash,
        })
    }
}
