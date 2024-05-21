use mempool_infra::network_component::NetworkComponent;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::{Tip, TransactionHash};
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThinTransaction {
    pub contract_address: ContractAddress,
    pub tx_hash: TransactionHash,
    pub tip: Tip,
    pub nonce: Nonce,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AccountState {
    pub nonce: Nonce,
    // TODO: add balance field when needed.
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Account {
    // TODO(Ayelet): Consider removing this field as it is duplicated in ThinTransaction.
    pub address: ContractAddress,
    pub state: AccountState,
}

#[derive(Debug, Default)]
pub struct MempoolInput {
    pub tx: ThinTransaction,
    pub account: Account,
}

#[derive(Debug)]
pub enum GatewayToMempoolMessage {
    AddTransaction(MempoolInput),
}

// TODO: Consider using `NetworkComponent` instead of defining the channels here.
// Currently, facing technical issues when using `NetworkComponent`.
pub struct BatcherToMempoolChannels {
    pub rx: Receiver<BatcherToMempoolMessage>,
    pub tx: Sender<MempoolToBatcherMessage>,
}

pub enum BatcherToMempoolMessage {
    GetTransactions(usize),
}
pub type MempoolToGatewayMessage = ();
pub type MempoolToBatcherMessage = Vec<ThinTransaction>;

pub type BatcherMempoolNetworkComponent =
    NetworkComponent<BatcherToMempoolMessage, MempoolToBatcherMessage>;
pub type MempoolBatcherNetworkComponent =
    NetworkComponent<MempoolToBatcherMessage, BatcherToMempoolMessage>;

pub type GatewayNetworkComponent =
    NetworkComponent<GatewayToMempoolMessage, MempoolToGatewayMessage>;
pub type MempoolNetworkComponent =
    NetworkComponent<MempoolToGatewayMessage, GatewayToMempoolMessage>;
