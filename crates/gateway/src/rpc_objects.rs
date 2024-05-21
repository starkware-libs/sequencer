use serde::{Deserialize, Serialize};
use serde_json::Value;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;

// Starknet Spec error codes:
// TODO(yael 30/4/2024): consider turning these into an enum.
pub const RPC_ERROR_BLOCK_NOT_FOUND: u16 = 24;
pub const RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND: u16 = 20;

#[derive(Deserialize, Serialize)]
pub enum BlockId {
    #[serde(rename = "block_number")]
    Number(BlockNumber),
    // There are additional options in the spec that are not implemented here
}

#[derive(Serialize, Deserialize)]
pub struct GetNonceParams {
    pub block_id: BlockId,
    pub contract_address: ContractAddress,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum RpcResponse {
    Success(RpcSuccessResponse),
    Error(RpcErrorResponse),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcSuccessResponse {
    pub jsonrpc: Option<String>,
    pub result: Value,
    pub id: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcErrorResponse {
    pub jsonrpc: Option<String>,
    pub error: RpcSpecError,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcSpecError {
    pub code: u16,
    pub message: String,
}
