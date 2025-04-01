use apollo_network_types::network_types::BroadcastedMessageMetadata;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::errors::GatewayError;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayInput {
    pub rpc_tx: RpcTransaction,
    pub message_metadata: Option<BroadcastedMessageMetadata>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GatewayOutput {
    Declare(DeclareGatewayOutput),
    DeployAccount(DeployAccountGatewayOutput),
    Invoke(InvokeGatewayOutput),
}

impl GatewayOutput {
    pub fn transaction_hash(&self) -> TransactionHash {
        match self {
            GatewayOutput::Declare(output) => output.transaction_hash,
            GatewayOutput::DeployAccount(output) => output.transaction_hash,
            GatewayOutput::Invoke(output) => output.transaction_hash,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeclareGatewayOutput {
    pub transaction_hash: TransactionHash,
    pub class_hash: ClassHash,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeployAccountGatewayOutput {
    pub transaction_hash: TransactionHash,
    pub address: ContractAddress,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InvokeGatewayOutput {
    pub transaction_hash: TransactionHash,
}

pub type GatewayResult<T> = Result<T, GatewayError>;
