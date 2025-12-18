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
use strum_macros::IntoStaticStr;

/// Storage reader request types for low-level database and file access.
#[derive(Clone, Debug, Serialize, Deserialize, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
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

#[cfg(test)]
#[path = "storage_reader_communication_test.rs"]
mod storage_reader_communication_test;
