use blockifier::execution::contract_class::ContractClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use reqwest::blocking::Client as BlockingClient;
use serde::Serialize;
use serde_json::{json, Value};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;
use url::Url;

use crate::rpc_objects::{
    BlockId, GetClassHashAtParams, GetNonceParams, GetStorageAtParams, RpcResponse,
    RPC_ERROR_BLOCK_NOT_FOUND, RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND,
};

pub struct RpcStateReader {
    pub url: Url,
    pub json_rpc_version: String,
    pub block_number: BlockNumber,
}

impl RpcStateReader {
    // Note: This function is blocking though it is sending a request to the rpc server and waiting
    // for the response.
    pub fn send_rpc_request<T: Serialize>(
        &self,
        method: &str,
        params: T,
    ) -> Result<Value, StateError> {
        let request_body = json!({
            "jsonrpc": self.json_rpc_version,
            "id": 0,
            "method": method,
            "params": json!(params),
        });

        let client = BlockingClient::new();
        let response = client
            .post(self.url.clone())
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
}

impl StateReader for RpcStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<StarkFelt> {
        let get_storage_at_params = GetStorageAtParams {
            block_id: BlockId::Number(self.block_number),
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
            block_id: BlockId::Number(self.block_number),
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
            block_id: BlockId::Number(self.block_number),
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
