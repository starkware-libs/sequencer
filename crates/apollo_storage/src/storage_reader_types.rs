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
use crate::header::StorageBlockHeader;
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
    /// Request the location in file for a state diff at a given block number.
    StateDiffsLocation(BlockNumber),
    /// Request a thin state diff from a specific location in file.
    StateDiffsFromLocation(LocationInFile),
    /// Request storage value at a contract address and key at a specific block.
    ContractStorage((ContractAddress, StorageKey), BlockNumber),
    /// Request nonce for a contract at a specific block.
    Nonces(ContractAddress, BlockNumber),
    /// Request class hash for a deployed contract at a specific block.
    DeployedContracts(ContractAddress, BlockNumber),
    /// Request if an event exists at a given contract address and transaction index.
    Events(ContractAddress, TransactionIndex),
    /// Request a marker by kind.
    Markers(MarkerKind),

    // ============ Class-Related Requests ============
    /// Request the location in file for a declared class (Sierra).
    DeclaredClassesLocation(ClassHash),
    /// Request a Sierra contract class from a specific location in file.
    DeclaredClassesFromLocation(LocationInFile),
    /// Request the block number when a class was declared.
    DeclaredClassesBlock(ClassHash),
    /// Request the location in file for a deprecated contract class.
    DeprecatedDeclaredClassesLocation(ClassHash),
    /// Request a deprecated contract class from a specific location in file.
    DeprecatedDeclaredClassesFromLocation(LocationInFile),
    /// Request the block number when a deprecated class was first declared.
    DeprecatedDeclaredClassesBlock(ClassHash),
    /// Request the location in file for a CASM contract class.
    CasmsLocation(ClassHash),
    /// Request a CASM contract class from a specific location in file.
    CasmsFromLocation(LocationInFile),
    /// Request compiled class hash at a specific block.
    CompiledClassHash(ClassHash, BlockNumber),
    /// Request stateless compiled class hash (v2).
    StatelessCompiledClassHashV2(ClassHash),

    // ============ Block-Related Requests ============
    /// Request a block header by block number.
    Headers(BlockNumber),
    /// Request block number by block hash.
    BlockHashToNumber(BlockHash),
    /// Request block signature by block number.
    BlockSignatures(BlockNumber),

    // ============ Transaction-Related Requests ============
    /// Request transaction metadata by transaction index.
    TransactionMetadata(TransactionIndex),
    /// Request transaction index by transaction hash.
    TransactionHashToIdx(TransactionHash),

    // ============ Other Requests ============
    /// Request the last voted marker.
    LastVotedMarker,
    /// Request file offset by offset kind.
    FileOffsets(OffsetKind),
    /// Request Starknet version by block number.
    StarknetVersion(BlockNumber),
    /// Request storage version by version name.
    StorageVersion(String),
}

/// Storage-related response.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StorageReaderResponse {
    // ============ State-Related Responses ============
    /// Response containing the location of a state diff in file.
    StateDiffsLocation(LocationInFile),
    /// Response containing a thin state diff.
    StateDiffsFromLocation(ThinStateDiff),
    /// Response containing a storage value.
    ContractStorage(Felt),
    /// Response containing a nonce.
    Nonces(Nonce),
    /// Response containing a class hash.
    DeployedContracts(ClassHash),
    /// Response indicating whether an event exists (unit value).
    Events,
    /// Response containing a marker block number.
    Markers(BlockNumber),

    // ============ Class-Related Responses ============
    /// Response containing the location of a declared class in file.
    DeclaredClassesLocation(LocationInFile),
    /// Response containing a Sierra contract class.
    DeclaredClassesFromLocation(SierraContractClass),
    /// Response containing the block number when a class was declared.
    DeclaredClassesBlock(BlockNumber),
    /// Response containing the location of a deprecated contract class in file.
    DeprecatedDeclaredClassesLocation(LocationInFile),
    /// Response containing a deprecated contract class.
    DeprecatedDeclaredClassesFromLocation(DeprecatedContractClass),
    /// Response containing the block number when a deprecated class was first declared.
    DeprecatedDeclaredClassesBlock(BlockNumber),
    /// Response containing the location of a CASM in file.
    CasmsLocation(LocationInFile),
    /// Response containing a CASM contract class.
    CasmsFromLocation(CasmContractClass),
    /// Response containing a compiled class hash.
    CompiledClassHash(CompiledClassHash),
    /// Response containing a stateless compiled class hash (v2).
    StatelessCompiledClassHashV2(CompiledClassHash),

    // ============ Block-Related Responses ============
    /// Response containing a block header.
    Headers(StorageBlockHeader),
    /// Response containing a block number.
    BlockHashToNumber(BlockNumber),
    /// Response containing a block signature.
    BlockSignatures(BlockNumber),

    // ============ Transaction-Related Responses ============
    /// Response containing transaction metadata.
    TransactionMetadata(TransactionMetadata),
    /// Response containing a transaction index.
    TransactionHashToIdx(TransactionIndex),

    // ============ Other Responses ============
    /// Response containing the last voted marker.
    LastVotedMarker(LastVotedMarker),
    /// Response containing a file offset.
    FileOffsets(usize),
    /// Response containing a Starknet version.
    StarknetVersion(StarknetVersion),
    /// Response containing a storage version.
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
            StorageReaderRequest::BlockHashToNumber(_block_hash) => {
                unimplemented!()
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
