use crate::body::TransactionIndex;
use crate::mmap_file::LocationInFile;
use crate::{IndexedDeprecatedContractClass, MarkerKind, StorageResult};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{SierraContractClass, StorageKey, ThinStateDiff};
use starknet_api::transaction::{Transaction, TransactionHash, TransactionOutput};
use starknet_types_core::felt::Felt;

/// Storage reader request types for low-level database and file access.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StorageReaderRequest {
    GetStateDiffLocation(BlockNumber),
    GetStateDiffFromFile(LocationInFile),
    
    GetClassLocation(ClassHash),
    GetClassFromFile(LocationInFile),
    GetClassDeclarationBlock(ClassHash),
    
    GetDeprecatedClassData(ClassHash),
    GetDeprecatedClassFromFile(LocationInFile),
    GetDeprecatedClassDeclarationBlock(ClassHash),
    
    GetCasmLocation(ClassHash),
    GetCasmFromFile(LocationInFile),
    GetExecutableClassHash(ClassHash),
    
    GetDeployedContractClassHash(ContractAddress, BlockNumber),
    GetContractStorageValue(ContractAddress, StorageKey, BlockNumber),
    GetNonceAtBlock(ContractAddress, BlockNumber),
    
    GetBlockNumberByHash(BlockHash),
    GetBlockSignatureByNumber(BlockNumber),
    
    GetTransactionLocation(TransactionIndex),
    GetTransactionFromFile(LocationInFile),
    GetTransactionIndexByHash(TransactionHash),
    GetTransactionOutputLocation(TransactionIndex),
    GetTransactionOutputFromFile(LocationInFile),
    
    GetMarker(MarkerKind),
}

/// Storage reader response types for low-level database and file access.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StorageReaderResponse {
    GetStateDiffLocation(StorageResult<Option<LocationInFile>>),
    GetStateDiffFromFile(StorageResult<ThinStateDiff>),
    
    GetClassLocation(StorageResult<Option<LocationInFile>>),
    GetClassFromFile(StorageResult<SierraContractClass>),
    GetClassDeclarationBlock(StorageResult<Option<BlockNumber>>),
    
    GetDeprecatedClassData(StorageResult<Option<IndexedDeprecatedContractClass>>),
    GetDeprecatedClassFromFile(StorageResult<DeprecatedContractClass>),
    GetDeprecatedClassDeclarationBlock(StorageResult<Option<BlockNumber>>),
    
    GetCasmLocation(StorageResult<Option<LocationInFile>>),
    GetCasmFromFile(StorageResult<CasmContractClass>),
    GetExecutableClassHash(StorageResult<Option<CompiledClassHash>>),
    
    GetDeployedContractClassHash(StorageResult<Option<ClassHash>>),
    GetContractStorageValue(StorageResult<Option<Felt>>),
    GetNonceAtBlock(StorageResult<Option<Nonce>>),
    
    GetBlockNumberByHash(StorageResult<Option<BlockNumber>>),
    GetBlockSignatureByNumber(StorageResult<Option<BlockSignature>>),
    
    GetTransactionLocation(StorageResult<Option<LocationInFile>>),
    GetTransactionFromFile(StorageResult<Transaction>),
    GetTransactionIndexByHash(StorageResult<Option<TransactionIndex>>),
    GetTransactionOutputLocation(StorageResult<Option<LocationInFile>>),
    GetTransactionOutputFromFile(StorageResult<TransactionOutput>),
    
    GetMarker(StorageResult<BlockNumber>),
}

impl StorageReaderRequest {
    /// Returns a human-readable description of the request type for logging and metrics.
    pub fn description(&self) -> &'static str {
        match self {
            Self::GetStateDiffLocation(_) => "get_state_diff_location",
            Self::GetStateDiffFromFile(_) => "get_state_diff_from_file",
            Self::GetClassLocation(_) => "get_class_location",
            Self::GetClassFromFile(_) => "get_class_from_file",
            Self::GetClassDeclarationBlock(_) => "get_class_declaration_block",
            Self::GetDeprecatedClassData(_) => "get_deprecated_class_data",
            Self::GetDeprecatedClassFromFile(_) => "get_deprecated_class_from_file",
            Self::GetDeprecatedClassDeclarationBlock(_) => {
                "get_deprecated_class_declaration_block"
            }
            Self::GetCasmLocation(_) => "get_casm_location",
            Self::GetCasmFromFile(_) => "get_casm_from_file",
            Self::GetExecutableClassHash(_) => "get_executable_class_hash",
            Self::GetDeployedContractClassHash(_, _) => "get_deployed_contract_class_hash",
            Self::GetContractStorageValue(_, _, _) => "get_contract_storage_value",
            Self::GetNonceAtBlock(_, _) => "get_nonce_at_block",
            Self::GetBlockNumberByHash(_) => "get_block_number_by_hash",
            Self::GetBlockSignatureByNumber(_) => "get_block_signature_by_number",
            Self::GetTransactionLocation(_) => "get_transaction_location",
            Self::GetTransactionFromFile(_) => "get_transaction_from_file",
            Self::GetTransactionIndexByHash(_) => "get_transaction_index_by_hash",
            Self::GetTransactionOutputLocation(_) => "get_transaction_output_location",
            Self::GetTransactionOutputFromFile(_) => "get_transaction_output_from_file",
            Self::GetMarker(_) => "get_marker",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_description() {
        let request = StorageReaderRequest::GetStateDiffLocation(BlockNumber(0));
        assert_eq!(request.description(), "get_state_diff_location");
    }

    #[test]
    fn test_serialization_deserialization() {
        // TODO: Add comprehensive serialization/deserialization tests for all request/response types
    }
}

