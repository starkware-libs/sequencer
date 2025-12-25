#[cfg(test)]
#[path = "storage_reader_types_test.rs"]
mod storage_reader_types_test;

use async_trait::async_trait;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, StarknetVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{SierraContractClass, StorageKey, ThinStateDiff};
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

use crate::body::TransactionIndex;
use crate::consensus::LastVotedMarker;
use crate::header::{HeaderStorageReader, StorageBlockHeader};
use crate::mmap_file::LocationInFile;
use crate::state::StateStorageReader;
use crate::storage_reader_server::{StorageReaderServer, StorageReaderServerHandler};
use crate::version::Version;
use crate::{MarkerKind, OffsetKind, StorageError, StorageReader, TransactionMetadata};

/// Type alias for the generic storage reader server.
pub type GenericStorageReaderServer = StorageReaderServer<
    GenericStorageReaderServerHandler,
    StorageReaderRequest,
    StorageReaderResponse,
>;

/// Storage-related requests.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StorageReaderRequest {
    // ============ State-Related Requests ============
    /// The location in file for a state diff at a given block number.
    StateDiffsLocation(BlockNumber),
    /// A thin state diff from a specific location in file.
    StateDiffsFromLocation(LocationInFile),
    /// Storage value at a contract address and key at a specific block.
    ContractStorage((ContractAddress, StorageKey), BlockNumber),
    /// Nonce for a contract at a specific block.
    Nonces(ContractAddress, BlockNumber),
    /// Class hash for a deployed contract at a specific block.
    DeployedContracts(ContractAddress, BlockNumber),
    /// If an event exists at a given contract address and transaction index.
    Events(ContractAddress, TransactionIndex),
    /// A marker by kind.
    Markers(MarkerKind),

    // ============ Class-Related Requests ============
    /// The location in file for a declared class (Sierra).
    DeclaredClassesLocation(ClassHash),
    /// A Sierra contract class from a specific location in file.
    DeclaredClassesFromLocation(LocationInFile),
    /// The block number when a class was declared.
    DeclaredClassesBlock(ClassHash),
    /// The location in file for a deprecated contract class.
    DeprecatedDeclaredClassesLocation(ClassHash),
    /// A deprecated contract class from a specific location in file.
    DeprecatedDeclaredClassesFromLocation(LocationInFile),
    /// The block number when a deprecated class was first declared.
    DeprecatedDeclaredClassesBlock(ClassHash),
    /// The location in file for a CASM contract class.
    CasmsLocation(ClassHash),
    /// A CASM contract class from a specific location in file.
    CasmsFromLocation(LocationInFile),
    /// Compiled class hash at a specific block.
    CompiledClassHash(ClassHash, BlockNumber),
    /// Stateless compiled class hash (v2).
    StatelessCompiledClassHashV2(ClassHash),

    // ============ Block-Related Requests ============
    /// A block header by block number.
    Headers(BlockNumber),
    /// Block number by block hash.
    BlockHashToNumber(BlockHash),
    /// Block signature by block number.
    BlockSignatures(BlockNumber),

    // ============ Transaction-Related Requests ============
    /// Transaction metadata by transaction index.
    TransactionMetadata(TransactionIndex),
    /// Transaction index by transaction hash.
    TransactionHashToIdx(TransactionHash),

    // ============ Other Requests ============
    /// The last voted marker.
    LastVotedMarker,
    /// File offset by offset kind.
    FileOffsets(OffsetKind),
    /// Starknet version by block number.
    StarknetVersion(BlockNumber),
    /// Storage version by version name.
    StorageVersion(String),
}

/// Storage-related response.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StorageReaderResponse {
    // ============ State-Related Responses ============
    /// The location of a state diff in file.
    StateDiffsLocation(LocationInFile),
    /// A thin state diff.
    StateDiffsFromLocation(ThinStateDiff),
    /// A storage value.
    ContractStorage(Felt),
    /// A nonce.
    Nonces(Nonce),
    /// A class hash.
    DeployedContracts(ClassHash),
    /// Indicates whether an event exists (unit value).
    Events,
    /// A marker block number.
    Markers(BlockNumber),

    // ============ Class-Related Responses ============
    /// The location of a declared class in file.
    DeclaredClassesLocation(LocationInFile),
    /// A Sierra contract class.
    DeclaredClassesFromLocation(SierraContractClass),
    /// The block number when a class was declared.
    DeclaredClassesBlock(BlockNumber),
    /// The location of a deprecated contract class in file.
    DeprecatedDeclaredClassesLocation(LocationInFile),
    /// A deprecated contract class.
    DeprecatedDeclaredClassesFromLocation(DeprecatedContractClass),
    /// The block number when a deprecated class was first declared.
    DeprecatedDeclaredClassesBlock(BlockNumber),
    /// The location of a CASM in file.
    CasmsLocation(LocationInFile),
    /// A CASM contract class.
    CasmsFromLocation(CasmContractClass),
    /// A compiled class hash.
    CompiledClassHash(CompiledClassHash),
    /// A stateless compiled class hash (v2).
    StatelessCompiledClassHashV2(CompiledClassHash),

    // ============ Block-Related Responses ============
    /// A block header.
    Headers(StorageBlockHeader),
    /// A block number.
    BlockHashToNumber(BlockNumber),
    /// A block signature.
    BlockSignatures(BlockNumber),

    // ============ Transaction-Related Responses ============
    /// Transaction metadata.
    TransactionMetadata(TransactionMetadata),
    /// A transaction index.
    TransactionHashToIdx(TransactionIndex),

    // ============ Other Responses ============
    /// The last voted marker.
    LastVotedMarker(LastVotedMarker),
    /// A file offset.
    FileOffsets(usize),
    /// A Starknet version.
    StarknetVersion(StarknetVersion),
    /// A storage version.
    StorageVersion(Version),
}

/// Generic handler for storage reader requests.
pub struct GenericStorageReaderServerHandler;

#[async_trait]
impl StorageReaderServerHandler<StorageReaderRequest, StorageReaderResponse>
    for GenericStorageReaderServerHandler
{
    async fn handle_request(
        storage_reader: &StorageReader,
        request: StorageReaderRequest,
    ) -> Result<StorageReaderResponse, StorageError> {
        let txn = storage_reader.begin_ro_txn()?;
        match request {
            // ============ State-Related Requests ============
            StorageReaderRequest::StateDiffsLocation(block_number) => {
                let state_diff_location =
                    txn.get_state_diff_location(block_number)?.ok_or(StorageError::NotFound {
                        resource_type: "State diff".to_string(),
                        resource_id: block_number.to_string(),
                    })?;
                Ok(StorageReaderResponse::StateDiffsLocation(state_diff_location))
            }
            StorageReaderRequest::StateDiffsFromLocation(location) => {
                let state_diff = txn.get_state_diff_from_location(location)?;
                Ok(StorageReaderResponse::StateDiffsFromLocation(state_diff))
            }
            StorageReaderRequest::ContractStorage(_key, _block_number) => {
                unimplemented!()
            }
            StorageReaderRequest::Nonces(_address, _block_number) => {
                unimplemented!()
            }
            StorageReaderRequest::DeployedContracts(_address, _block_number) => {
                unimplemented!()
            }
            StorageReaderRequest::Events(_address, _tx_index) => {
                unimplemented!()
            }
            StorageReaderRequest::Markers(marker_kind) => {
                let block_number = match marker_kind {
                    MarkerKind::State => txn.get_state_marker()?,
                    _ => unimplemented!(),
                };
                Ok(StorageReaderResponse::Markers(block_number))
            }

            // ============ Class-Related Requests ============
            StorageReaderRequest::DeclaredClassesLocation(_class_hash) => {
                unimplemented!()
            }
            StorageReaderRequest::DeclaredClassesFromLocation(_location) => {
                unimplemented!()
            }
            StorageReaderRequest::DeclaredClassesBlock(_class_hash) => {
                unimplemented!()
            }
            StorageReaderRequest::DeprecatedDeclaredClassesLocation(_class_hash) => {
                unimplemented!()
            }
            StorageReaderRequest::DeprecatedDeclaredClassesFromLocation(_location) => {
                unimplemented!()
            }
            StorageReaderRequest::DeprecatedDeclaredClassesBlock(_class_hash) => {
                unimplemented!()
            }
            StorageReaderRequest::CasmsLocation(_class_hash) => {
                unimplemented!()
            }
            StorageReaderRequest::CasmsFromLocation(_location) => {
                unimplemented!()
            }
            StorageReaderRequest::CompiledClassHash(_class_hash, _block_number) => {
                unimplemented!()
            }
            StorageReaderRequest::StatelessCompiledClassHashV2(_class_hash) => {
                unimplemented!()
            }

            // ============ Block-Related Requests ============
            StorageReaderRequest::Headers(_block_number) => {
                unimplemented!()
            }
            StorageReaderRequest::BlockHashToNumber(block_hash) => {
                let block_number =
                    txn.get_block_number_by_hash(&block_hash)?.ok_or(StorageError::NotFound {
                        resource_type: "Block number".to_string(),
                        resource_id: format!("hash: {}", block_hash),
                    })?;
                Ok(StorageReaderResponse::BlockHashToNumber(block_number))
            }
            StorageReaderRequest::BlockSignatures(_block_number) => {
                unimplemented!()
            }

            // ============ Transaction-Related Requests ============
            StorageReaderRequest::TransactionMetadata(_tx_index) => {
                unimplemented!()
            }
            StorageReaderRequest::TransactionHashToIdx(_tx_hash) => {
                unimplemented!()
            }

            // ============ Other Requests ============
            StorageReaderRequest::LastVotedMarker => {
                unimplemented!()
            }
            StorageReaderRequest::FileOffsets(_offset_kind) => {
                unimplemented!()
            }
            StorageReaderRequest::StarknetVersion(_block_number) => {
                unimplemented!()
            }
            StorageReaderRequest::StorageVersion(_version_name) => {
                unimplemented!()
            }
        }
    }
}
