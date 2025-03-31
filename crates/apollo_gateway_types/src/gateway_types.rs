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
    Declare(DeclareSpecificGatewayOutput),
    DeployAccount(DeployAccountSpecificGatewayOutput),
    Invoke(InvokeSpecificGatewayOutput),
}

impl GatewayOutput {
    pub fn tx_hash(&self) -> TransactionHash {
        match self {
            GatewayOutput::Declare(output) => output.tx_hash,
            GatewayOutput::DeployAccount(output) => output.tx_hash,
            GatewayOutput::Invoke(output) => output.tx_hash,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeclareSpecificGatewayOutput {
    pub tx_hash: TransactionHash,
    pub class_hash: ClassHash,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeployAccountSpecificGatewayOutput {
    pub tx_hash: TransactionHash,
    pub contract_address: ContractAddress,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InvokeSpecificGatewayOutput {
    pub tx_hash: TransactionHash,
}

pub type GatewayResult<T> = Result<T, GatewayError>;
