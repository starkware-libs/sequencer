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

pub fn create_internal_tx(
    sender_address: ContractAddress,
    tx_hash: TransactionHash,
    tip: Tip,
    nonce: Nonce,
) -> Transaction {
    Transaction::Invoke(InvokeTransaction {
        tx: starknet_api::transaction::InvokeTransaction::V3(
            starknet_api::transaction::InvokeTransactionV3 {
                sender_address,
                tip,
                nonce,
                resource_bounds: ResourceBoundsMapping::default(),
                signature: TransactionSignature::default(),
                calldata: Calldata::default(),
                nonce_data_availability_mode: DataAvailabilityMode::L1,
                fee_data_availability_mode: DataAvailabilityMode::L2,
                paymaster_data: PaymasterData::default(),
                account_deployment_data: AccountDeploymentData::default(),
            },
        ),
        tx_hash,
    })
}
