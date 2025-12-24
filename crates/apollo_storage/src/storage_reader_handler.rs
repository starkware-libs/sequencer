use tracing::debug;

use crate::storage_reader::StorageReaderApi;
use crate::storage_reader_communication::{StorageReaderRequest, StorageReaderResponse};
use crate::{StorageError, StorageReader};

/// Unified handler for storage reader requests.
///
/// This handler can be used by any component that needs to access storage
/// via the StorageReaderRequest/Response pattern.
pub struct StorageReaderHandler {
    storage_reader: StorageReader,
}

impl StorageReaderHandler {
    /// Creates a new storage reader handler.
    pub fn new(storage_reader: StorageReader) -> Self {
        Self { storage_reader }
    }

    /// Handles a storage reader request and returns the appropriate response.
    ///
    /// This method:
    /// 1. Opens a read-only transaction
    /// 2. Dispatches the request to the appropriate storage API method
    /// 3. Returns the result wrapped in a `StorageReaderResponse`
    ///
    /// Errors are propagated via the `Result` type.
    pub fn handle_request(
        &self,
        request: StorageReaderRequest,
    ) -> Result<StorageReaderResponse, StorageError> {
        let description: &'static str = (&request).into();
        debug!("Handling storage reader request: {}", description);

        let txn = self.storage_reader.begin_ro_txn()?;

        // Dispatch the request to the appropriate handler.
        match request {
            StorageReaderRequest::GetStateDiffLocation(block_number) => {
                let result = txn.get_state_diff_location(block_number)?;
                Ok(StorageReaderResponse::GetStateDiffLocation(result))
            }
            StorageReaderRequest::GetStateDiffFromFile(location) => {
                let result = txn.get_state_diff_from_file(location)?;
                Ok(StorageReaderResponse::GetStateDiffFromFile(result))
            }

            StorageReaderRequest::GetClassLocation(class_hash) => {
                let result = txn.get_class_location(&class_hash)?;
                Ok(StorageReaderResponse::GetClassLocation(result))
            }
            StorageReaderRequest::GetClassFromFile(location) => {
                let result = txn.get_class_from_file(location)?;
                Ok(StorageReaderResponse::GetClassFromFile(result))
            }
            StorageReaderRequest::GetClassDeclarationBlock(class_hash) => {
                let result = txn.get_class_declaration_block(&class_hash)?;
                Ok(StorageReaderResponse::GetClassDeclarationBlock(result))
            }

            StorageReaderRequest::GetDeprecatedClassData(class_hash) => {
                let result = txn.get_deprecated_class_data(&class_hash)?;
                Ok(StorageReaderResponse::GetDeprecatedClassData(result))
            }
            StorageReaderRequest::GetDeprecatedClassFromFile(location) => {
                let result = txn.get_deprecated_class_from_file(location)?;
                Ok(StorageReaderResponse::GetDeprecatedClassFromFile(result))
            }
            StorageReaderRequest::GetDeprecatedClassDeclarationBlock(class_hash) => {
                let result = txn.get_deprecated_class_declaration_block(&class_hash)?;
                Ok(StorageReaderResponse::GetDeprecatedClassDeclarationBlock(result))
            }

            StorageReaderRequest::GetCasmLocation(class_hash) => {
                let result = txn.get_casm_location(&class_hash)?;
                Ok(StorageReaderResponse::GetCasmLocation(result))
            }
            StorageReaderRequest::GetCasmFromFile(location) => {
                let result = txn.get_casm_from_file(location)?;
                Ok(StorageReaderResponse::GetCasmFromFile(result))
            }
            StorageReaderRequest::GetExecutableClassHash(class_hash) => {
                let result = txn.get_executable_class_hash(&class_hash)?;
                Ok(StorageReaderResponse::GetExecutableClassHash(result))
            }

            StorageReaderRequest::GetDeployedContractClassHash(address, block_number) => {
                let result = txn.get_deployed_contract_class_hash(&address, block_number)?;
                Ok(StorageReaderResponse::GetDeployedContractClassHash(result))
            }
            StorageReaderRequest::GetContractStorageValue(address, storage_key, block_number) => {
                let result =
                    txn.get_contract_storage_value(&address, &storage_key, block_number)?;
                Ok(StorageReaderResponse::GetContractStorageValue(result))
            }
            StorageReaderRequest::GetNonceAtBlock(address, block_number) => {
                let result = txn.get_nonce_at_block(&address, block_number)?;
                Ok(StorageReaderResponse::GetNonceAtBlock(result))
            }

            StorageReaderRequest::GetBlockNumberByHash(block_hash) => {
                let result = txn.get_block_number_by_hash(&block_hash)?;
                Ok(StorageReaderResponse::GetBlockNumberByHash(result))
            }
            StorageReaderRequest::GetBlockSignatureByNumber(block_number) => {
                let result = txn.get_block_signature_by_number(block_number)?;
                Ok(StorageReaderResponse::GetBlockSignatureByNumber(result))
            }

            StorageReaderRequest::GetTransactionLocation(transaction_index) => {
                let result = txn.get_transaction_location(transaction_index)?;
                Ok(StorageReaderResponse::GetTransactionLocation(result))
            }
            StorageReaderRequest::GetTransactionFromFile(location) => {
                let result = txn.get_transaction_from_file(location)?;
                Ok(StorageReaderResponse::GetTransactionFromFile(result))
            }
            StorageReaderRequest::GetTransactionIndexByHash(tx_hash) => {
                let result = txn.get_transaction_index_by_hash(&tx_hash)?;
                Ok(StorageReaderResponse::GetTransactionIndexByHash(result))
            }
            StorageReaderRequest::GetTransactionOutputLocation(transaction_index) => {
                let result = txn.get_transaction_output_location(transaction_index)?;
                Ok(StorageReaderResponse::GetTransactionOutputLocation(result))
            }
            StorageReaderRequest::GetTransactionOutputFromFile(location) => {
                let result = txn.get_transaction_output_from_file(location)?;
                Ok(StorageReaderResponse::GetTransactionOutputFromFile(result))
            }

            StorageReaderRequest::GetMarker(marker_kind) => {
                let result = txn.get_marker(marker_kind)?;
                Ok(StorageReaderResponse::GetMarker(result))
            }
        }
    }
}

#[cfg(test)]
#[path = "storage_reader_handler_test.rs"]
mod tests;
