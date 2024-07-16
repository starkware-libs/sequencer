use blockifier::blockifier::block::BlockInfo;
use blockifier::execution::contract_class::{ContractClass, ContractClassV0, ContractClassV1};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use papyrus_rpc::CompiledContractClass;
use reqwest::blocking::Client as BlockingClient;
use serde::Serialize;
use serde_json::{json, Value};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::config::RpcStateReaderConfig;
use crate::errors::{serde_err_to_state_err, RPCStateReaderError, RPCStateReaderResult};
use crate::rpc_objects::{
    BlockHeader, BlockId, GetBlockWithTxHashesParams, GetClassHashAtParams,
    GetCompiledContractClassParams, GetNonceParams, GetStorageAtParams, RpcResponse,
    RPC_CLASS_HASH_NOT_FOUND, RPC_ERROR_BLOCK_NOT_FOUND, RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND,
};
use crate::state_reader::{MempoolStateReader, StateReaderFactory};

pub struct RpcStateReader {
    pub config: RpcStateReaderConfig,
    pub block_id: BlockId,
}

impl RpcStateReader {
    pub fn from_number(config: &RpcStateReaderConfig, block_number: BlockNumber) -> Self {
        Self { config: config.clone(), block_id: BlockId::Number(block_number) }
    }
    pub fn from_latest(config: &RpcStateReaderConfig) -> Self {
        Self { config: config.clone(), block_id: BlockId::Latest }
    }
    // Note: This function is blocking though it is sending a request to the rpc server and waiting
    // for the response.
    pub fn send_rpc_request(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> RPCStateReaderResult<Value> {
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
            .send()?;

        if !response.status().is_success() {
            return Err(RPCStateReaderError::RPCError(response.status()));
        }

        let rpc_response: RpcResponse = response.json::<RpcResponse>()?;

        match rpc_response {
            RpcResponse::Success(rpc_success_response) => Ok(rpc_success_response.result),
            RpcResponse::Error(rpc_error_response) => match rpc_error_response.error.code {
                RPC_ERROR_BLOCK_NOT_FOUND => Err(RPCStateReaderError::BlockNotFound(request_body)),
                RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND => {
                    Err(RPCStateReaderError::ContractAddressNotFound(request_body))
                }
                RPC_CLASS_HASH_NOT_FOUND => {
                    Err(RPCStateReaderError::ClassHashNotFound(request_body))
                }
                _ => Err(RPCStateReaderError::UnexpectedErrorCode(rpc_error_response.error.code)),
            },
        }
    }
}

impl MempoolStateReader for RpcStateReader {
    fn get_block_info(&self) -> StateResult<BlockInfo> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.block_id };

        // The response from the rpc is a full block but we only deserialize the header.
        let block_header: BlockHeader = serde_json::from_value(
            self.send_rpc_request("starknet_getBlockWithTxHashes", get_block_params)?,
        )
        .map_err(serde_err_to_state_err)?;
        let block_info = block_header.try_into()?;
        Ok(block_info)
    }
}

impl BlockifierStateReader for RpcStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        let get_storage_at_params =
            GetStorageAtParams { block_id: self.block_id, contract_address, key };

        let result = self.send_rpc_request("starknet_getStorageAt", get_storage_at_params)?;
        let value: Felt = serde_json::from_value(result).map_err(serde_err_to_state_err)?;
        Ok(value)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let get_nonce_params = GetNonceParams { block_id: self.block_id, contract_address };

        let result = self.send_rpc_request("starknet_getNonce", get_nonce_params);
        match result {
            Ok(value) => {
                let nonce: Nonce = serde_json::from_value(value).map_err(serde_err_to_state_err)?;
                Ok(nonce)
            }
            Err(RPCStateReaderError::ContractAddressNotFound(_)) => Ok(Nonce::default()),
            Err(e) => Err(e)?,
        }
    }

    fn get_compiled_contract_class(&self, class_hash: ClassHash) -> StateResult<ContractClass> {
        let get_compiled_class_params =
            GetCompiledContractClassParams { class_hash, block_id: self.block_id };

        let result =
            self.send_rpc_request("starknet_getCompiledContractClass", get_compiled_class_params)?;
        let contract_class: CompiledContractClass =
            serde_json::from_value(result).map_err(serde_err_to_state_err)?;
        match contract_class {
            CompiledContractClass::V1(contract_class_v1) => Ok(ContractClass::V1(
                ContractClassV1::try_from(contract_class_v1).map_err(StateError::ProgramError)?,
            )),
            CompiledContractClass::V0(contract_class_v0) => Ok(ContractClass::V0(
                ContractClassV0::try_from(contract_class_v0).map_err(StateError::ProgramError)?,
            )),
        }
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let get_class_hash_at_params =
            GetClassHashAtParams { contract_address, block_id: self.block_id };

        let result = self.send_rpc_request("starknet_getClassHashAt", get_class_hash_at_params);
        match result {
            Ok(value) => {
                let class_hash: ClassHash =
                    serde_json::from_value(value).map_err(serde_err_to_state_err)?;
                Ok(class_hash)
            }
            Err(RPCStateReaderError::ContractAddressNotFound(_)) => Ok(ClassHash::default()),
            Err(e) => Err(e)?,
        }
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}

pub struct RpcStateReaderFactory {
    pub config: RpcStateReaderConfig,
}

impl StateReaderFactory for RpcStateReaderFactory {
    fn get_state_reader_from_latest_block(&self) -> Box<dyn MempoolStateReader> {
        Box::new(RpcStateReader::from_latest(&self.config))
    }

    fn get_state_reader(&self, block_number: BlockNumber) -> Box<dyn MempoolStateReader> {
        Box::new(RpcStateReader::from_number(&self.config, block_number))
    }
}
