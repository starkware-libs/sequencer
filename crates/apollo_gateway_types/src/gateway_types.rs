use starknet_api::rpc_transaction::RpcTransaction;
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use serde::{Deserialize, Serialize};

use crate::errors::GatewayError;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayInput {
    pub rpc_tx: RpcTransaction,
    pub message_metadata: Option<BroadcastedMessageMetadata>,
}

pub type GatewayResult<T> = Result<T, GatewayError>;
