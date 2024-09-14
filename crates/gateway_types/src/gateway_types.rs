use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::RpcTransaction;

use crate::errors::GatewayError;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageMetadata {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayInput {
    pub rpc_tx: RpcTransaction,
    pub message_metadata: MessageMetadata,
}

pub type GatewayResult<T> = Result<T, GatewayError>;
