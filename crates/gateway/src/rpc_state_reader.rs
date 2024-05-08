use blockifier::blockifier::block::BlockInfo;
use blockifier::execution::contract_class::ContractClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::StateReader as BlockifierStateReader;
use blockifier::state::state_api::StateResult;
use reqwest::blocking::Client as BlockingClient;
use serde::Serialize;
use serde_json::{json, Value};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;
use url::Url;

use crate::rpc_objects::{
    BlockHeader, BlockId, GetBlockWithTxHashesParams, GetClassHashAtParams, GetNonceParams,
    GetStorageAtParams, RpcResponse, RPC_ERROR_BLOCK_NOT_FOUND,
    RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND,
};

pub struct RpcStateReader {
    pub config: RpcStateReaderConfig,
    pub block_id: BlockId,
}

impl RpcStateReader {
    pub fn from_number(config: &RpcStateReaderConfig, block_number: BlockNumber) -> Self {
        Self {
            config: config.clone(),
            block_id: BlockId::Number(block_number),
        }
    }
    pub fn from_latest(config: &RpcStateReaderConfig) -> Self {
        Self {
            config: config.clone(),
            block_id: BlockId::Latest,
        }
    }
    // Note: This function is blocking though it is sending a request to the rpc server and waiting
    // for the response.
    pub fn send_rpc_request(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> Result<Value, StateError> {
        let request_body = json!({
            "jsonrpc": self.config.json_rpc_version,
            "id": 0,
            "method": method,
            "params": json!(params),
        });

        let client = BlockingClient::new();
        let response = client
            .post(self.config.url.clone())
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .map_err(|e| {
                StateError::StateReadError(format!("Rpc request failed with error {:?}", e))
            })?;

        if !response.status().is_success() {
            return Err(StateError::StateReadError(format!(
                "RPC ERROR, code {}",
                response.status()
            )));
        }

        let rpc_response: RpcResponse = response.json::<RpcResponse>().map_err(|e| {
            StateError::StateReadError(format!("Couldn't parse json rpc response {}", e))
        })?;

        match rpc_response {
            RpcResponse::Success(rpc_success_response) => Ok(rpc_success_response.result),
            RpcResponse::Error(rpc_error_response) => match rpc_error_response.error.code {
                RPC_ERROR_BLOCK_NOT_FOUND => Err(StateError::StateReadError(format!(
                    "Block not found, request: {}",
                    request_body
                ))),
                RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND => Err(StateError::StateReadError(format!(
                    "Contract address not found, request: {}",
                    request_body
                ))),
                _ => Err(StateError::StateReadError(format!(
                    "Unexpected error code {}",
                    rpc_error_response.error.code
                ))),
            },
        }
    }

    pub fn get_block_info(&self) -> Result<BlockInfo, StateError> {
        let get_block_params = GetBlockWithTxHashesParams {
            block_id: self.block_id,
        };

        // The response from the rpc is a full block but we only deserialize the header.
        let block_header: BlockHeader = serde_json::from_value(
            self.send_rpc_request("starknet_getBlockWithTxHashes", get_block_params)?,
        )
        .map_err(|e| {
            StateError::StateReadError(format!("Couldn't parse block with tx hashes {}", e))
        })?;
        let block_info = block_header.try_into()?;
        Ok(block_info)
    }
}

impl BlockifierStateReader for RpcStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<StarkFelt> {
        let get_storage_at_params = GetStorageAtParams {
            block_id: self.block_id,
            contract_address,
            key,
        };

        let result = self.send_rpc_request("starknet_getStorageAt", get_storage_at_params)?;
        let value: StarkFelt = serde_json::from_value(result)
            .map_err(|_| StateError::StateReadError("Bad rpc result".to_string()))?;
        Ok(value)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let get_nonce_params = GetNonceParams {
            block_id: self.block_id,
            contract_address,
        };

        let result = self.send_rpc_request("starknet_getNonce", get_nonce_params)?;
        let nonce: Nonce = serde_json::from_value(result)
            .map_err(|_| StateError::StateReadError("Bad rpc result".to_string()))?;
        Ok(nonce)
    }

    #[allow(unused_variables)]
    fn get_compiled_contract_class(&self, class_hash: ClassHash) -> StateResult<ContractClass> {
        todo!()
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let get_class_hash_at_params = GetClassHashAtParams {
            contract_address,
            block_id: self.block_id,
        };

        let result = self.send_rpc_request("starknet_getClassHashAt", get_class_hash_at_params)?;
        let class_hash: ClassHash = serde_json::from_value(result)
            .map_err(|_| StateError::StateReadError("Bad rpc result".to_string()))?;
        Ok(class_hash)
    }

    #[allow(unused_variables)]
    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}

#[derive(Clone)]
pub struct RpcStateReaderConfig {
    pub url: Url,
    pub json_rpc_version: String,
}
