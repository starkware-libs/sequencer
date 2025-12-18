//! Storage reader communication types for remote HTTP queries.
//!
//! This module defines the request and response types used by the `StorageReaderServer`
//! to handle cross-process storage queries via HTTP/JSON.

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{SierraContractClass, StorageKey, ThinStateDiff};
use starknet_api::transaction::{Transaction, TransactionHash, TransactionOutput};
use starknet_types_core::felt::Felt;
use strum_macros::IntoStaticStr;

use crate::body::TransactionIndex;
use crate::mmap_file::LocationInFile;
use crate::{IndexedDeprecatedContractClass, MarkerKind};

/// Storage reader request types for low-level database and file access.
#[derive(Clone, Debug, Serialize, Deserialize, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum StorageReaderRequest {
    /// Get the file location of a state diff for a specific block.
    GetStateDiffLocation(BlockNumber),
    /// Read a state diff from a file at the given location.
    GetStateDiffFromFile(LocationInFile),

    /// Get the file location of a Sierra contract class.
    GetClassLocation(ClassHash),
    /// Read a Sierra contract class from a file at the given location.
    GetClassFromFile(LocationInFile),
    /// Get the block number where a class was declared.
    GetClassDeclarationBlock(ClassHash),

    /// Get indexed metadata for a deprecated (Cairo 0) contract class.
    GetDeprecatedClassData(ClassHash),
    /// Read a deprecated contract class from a file at the given location.
    GetDeprecatedClassFromFile(LocationInFile),
    /// Get the block number where a deprecated class was declared.
    GetDeprecatedClassDeclarationBlock(ClassHash),

    /// Get the file location of a compiled class (CASM).
    GetCasmLocation(ClassHash),
    /// Read a CASM from a file at the given location.
    GetCasmFromFile(LocationInFile),
    /// Get the executable (compiled) class hash for a Sierra class.
    GetExecutableClassHash(ClassHash),

    /// Get the class hash of a contract deployed at a specific address and block.
    GetDeployedContractClassHash(ContractAddress, BlockNumber),
    /// Get a storage value for a contract at a specific key and block.
    GetContractStorageValue(ContractAddress, StorageKey, BlockNumber),
    /// Get the nonce of a contract at a specific block.
    GetNonceAtBlock(ContractAddress, BlockNumber),

    /// Get the block number for a given block hash.
    GetBlockNumberByHash(BlockHash),
    /// Get the signature for a specific block number.
    GetBlockSignatureByNumber(BlockNumber),

    /// Get the file location of a transaction.
    GetTransactionLocation(TransactionIndex),
    /// Read a transaction from a file at the given location.
    GetTransactionFromFile(LocationInFile),
    /// Get the transaction index for a given transaction hash.
    GetTransactionIndexByHash(TransactionHash),
    /// Get the file location of a transaction output.
    GetTransactionOutputLocation(TransactionIndex),
    /// Read a transaction output from a file at the given location.
    GetTransactionOutputFromFile(LocationInFile),

    /// Get a storage marker indicating the first unprocessed block for a specific data type.
    GetMarker(MarkerKind),
}

/// Storage reader response types for low-level database and file access.
///
/// Note: These responses represent successful results. Errors are returned via the handler's
/// Result type and converted to HTTP error responses by the server framework.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StorageReaderResponse {
    /// File location of the state diff, or None if not found.
    GetStateDiffLocation(Option<LocationInFile>),
    /// State diff data read from file.
    GetStateDiffFromFile(ThinStateDiff),

    /// File location of the Sierra contract class, or None if not found.
    GetClassLocation(Option<LocationInFile>),
    /// Sierra contract class data read from file.
    GetClassFromFile(SierraContractClass),
    /// Block number where the class was declared, or None if not found.
    GetClassDeclarationBlock(Option<BlockNumber>),

    /// Indexed metadata for the deprecated class, or None if not found.
    GetDeprecatedClassData(Option<IndexedDeprecatedContractClass>),
    /// Deprecated contract class data read from file.
    GetDeprecatedClassFromFile(DeprecatedContractClass),
    /// Block number where the deprecated class was declared, or None if not found.
    GetDeprecatedClassDeclarationBlock(Option<BlockNumber>),

    /// File location of the CASM, or None if not found.
    GetCasmLocation(Option<LocationInFile>),
    /// CASM data read from file.
    GetCasmFromFile(CasmContractClass),
    /// Executable class hash, or None if not found.
    GetExecutableClassHash(Option<CompiledClassHash>),

    /// Class hash of the deployed contract, or None if not found.
    GetDeployedContractClassHash(Option<ClassHash>),
    /// Storage value, or None if not found.
    GetContractStorageValue(Option<Felt>),
    /// Contract nonce, or None if not found.
    GetNonceAtBlock(Option<Nonce>),

    /// Block number for the given hash, or None if not found.
    GetBlockNumberByHash(Option<BlockNumber>),
    /// Block signature, or None if not found.
    GetBlockSignatureByNumber(Option<BlockSignature>),

    /// File location of the transaction, or None if not found.
    GetTransactionLocation(Option<LocationInFile>),
    /// Transaction data read from file.
    GetTransactionFromFile(Transaction),
    /// Transaction index, or None if not found.
    GetTransactionIndexByHash(Option<TransactionIndex>),
    /// File location of the transaction output, or None if not found.
    GetTransactionOutputLocation(Option<LocationInFile>),
    /// Transaction output data read from file.
    GetTransactionOutputFromFile(TransactionOutput),

    /// Storage marker value indicating the first unprocessed block.
    GetMarker(BlockNumber),
}

#[cfg(test)]
#[path = "storage_reader_communication_test.rs"]
mod storage_reader_communication_test;
