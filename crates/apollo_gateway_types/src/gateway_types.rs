use apollo_network_types::network_types::BroadcastedMessageMetadata;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::errors::GatewayError;

const TRANSACTION_RECEIVED: &str = "TRANSACTION_RECEIVED";
// Version 4 exists only for deploy_account transactions (Blake2 address derivation); the
// per-type gate is the serde of the transaction enums — this list only shapes the fallback
// version error on the deprecated gateway endpoint.
pub const SUPPORTED_TRANSACTION_VERSIONS: [u64; 2] = [3, 4];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayInput {
    pub rpc_tx: RpcTransaction,
    pub message_metadata: Option<BroadcastedMessageMetadata>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
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
    code: String,
}

impl DeclareGatewayOutput {
    pub fn new(transaction_hash: TransactionHash, class_hash: ClassHash) -> Self {
        Self { transaction_hash, class_hash, code: TRANSACTION_RECEIVED.to_string() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeployAccountGatewayOutput {
    pub transaction_hash: TransactionHash,
    pub address: ContractAddress,
    code: String,
}

impl DeployAccountGatewayOutput {
    pub fn new(transaction_hash: TransactionHash, address: ContractAddress) -> Self {
        Self { transaction_hash, address, code: TRANSACTION_RECEIVED.to_string() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InvokeGatewayOutput {
    pub transaction_hash: TransactionHash,
    code: String,
}

impl InvokeGatewayOutput {
    pub fn new(transaction_hash: TransactionHash) -> Self {
        Self { transaction_hash, code: TRANSACTION_RECEIVED.to_string() }
    }
}

pub type GatewayResult<T> = Result<T, GatewayError>;
