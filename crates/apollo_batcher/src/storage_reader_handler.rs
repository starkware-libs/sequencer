use apollo_storage::storage_reader::StorageReaderApi;
use apollo_storage::storage_reader_communication::{StorageReaderRequest, StorageReaderResponse};
use apollo_storage::{StorageReader, StorageResult};
use tracing::{debug, error};

/// Handler for storage reader requests in the Batcher.
pub struct BatcherStorageReaderHandler {
    storage_reader: StorageReader,
}

impl BatcherStorageReaderHandler {
    /// Creates a new storage reader handler.
    pub fn new(storage_reader: StorageReader) -> Self {
        Self { storage_reader }
    }

    /// Handles a storage reader request and returns the appropriate response:
    /// 1. Opens a read-only transaction
    /// 2. Dispatches the request to the appropriate storage API method
    /// 3. Wraps the result in a StorageReaderResponse
    pub fn handle_request(&self, request: StorageReaderRequest) -> StorageReaderResponse {
        debug!("Handling storage reader request: {:?}", request.description());

        // Open a read-only transaction.
        let txn_result = self.storage_reader.begin_ro_txn();
        let txn = match txn_result {
            Ok(txn) => txn,
            Err(e) => {
                error!("Failed to open read transaction: {:?}", e);
                // Return error response for the specific request type.
                return self.error_response_for_request(&request, e);
            }
        };

        // Dispatch the request to the appropriate handler.
        match request {
            StorageReaderRequest::GetStateDiffLocation(block_number) => {
                StorageReaderResponse::GetStateDiffLocation(
                    txn.get_state_diff_location(block_number),
                )
            }
            StorageReaderRequest::GetStateDiffFromFile(location) => {
                StorageReaderResponse::GetStateDiffFromFile(txn.get_state_diff_from_file(location))
            }

            StorageReaderRequest::GetClassLocation(class_hash) => {
                StorageReaderResponse::GetClassLocation(txn.get_class_location(&class_hash))
            }
            StorageReaderRequest::GetClassFromFile(location) => {
                StorageReaderResponse::GetClassFromFile(txn.get_class_from_file(location))
            }
            StorageReaderRequest::GetClassDeclarationBlock(class_hash) => {
                StorageReaderResponse::GetClassDeclarationBlock(
                    txn.get_class_declaration_block(&class_hash),
                )
            }

            StorageReaderRequest::GetDeprecatedClassData(class_hash) => {
                StorageReaderResponse::GetDeprecatedClassData(
                    txn.get_deprecated_class_data(&class_hash),
                )
            }
            StorageReaderRequest::GetDeprecatedClassFromFile(location) => {
                StorageReaderResponse::GetDeprecatedClassFromFile(
                    txn.get_deprecated_class_from_file(location),
                )
            }
            StorageReaderRequest::GetDeprecatedClassDeclarationBlock(class_hash) => {
                StorageReaderResponse::GetDeprecatedClassDeclarationBlock(
                    txn.get_deprecated_class_declaration_block(&class_hash),
                )
            }

            StorageReaderRequest::GetCasmLocation(class_hash) => {
                StorageReaderResponse::GetCasmLocation(txn.get_casm_location(&class_hash))
            }
            StorageReaderRequest::GetCasmFromFile(location) => {
                StorageReaderResponse::GetCasmFromFile(txn.get_casm_from_file(location))
            }
            StorageReaderRequest::GetExecutableClassHash(class_hash) => {
                StorageReaderResponse::GetExecutableClassHash(
                    txn.get_executable_class_hash(&class_hash),
                )
            }

            StorageReaderRequest::GetDeployedContractClassHash(address, block_number) => {
                StorageReaderResponse::GetDeployedContractClassHash(
                    txn.get_deployed_contract_class_hash(&address, block_number),
                )
            }
            StorageReaderRequest::GetContractStorageValue(address, storage_key, block_number) => {
                StorageReaderResponse::GetContractStorageValue(
                    txn.get_contract_storage_value(&address, &storage_key, block_number),
                )
            }
            StorageReaderRequest::GetNonceAtBlock(address, block_number) => {
                StorageReaderResponse::GetNonceAtBlock(txn.get_nonce_at_block(&address, block_number))
            }

            StorageReaderRequest::GetBlockNumberByHash(block_hash) => {
                StorageReaderResponse::GetBlockNumberByHash(
                    txn.get_block_number_by_hash(&block_hash),
                )
            }
            StorageReaderRequest::GetBlockSignatureByNumber(block_number) => {
                StorageReaderResponse::GetBlockSignatureByNumber(
                    txn.get_block_signature_by_number(block_number),
                )
            }

            StorageReaderRequest::GetTransactionLocation(transaction_index) => {
                StorageReaderResponse::GetTransactionLocation(
                    txn.get_transaction_location(transaction_index),
                )
            }
            StorageReaderRequest::GetTransactionFromFile(location) => {
                StorageReaderResponse::GetTransactionFromFile(
                    txn.get_transaction_from_file(location),
                )
            }
            StorageReaderRequest::GetTransactionIndexByHash(tx_hash) => {
                StorageReaderResponse::GetTransactionIndexByHash(
                    txn.get_transaction_index_by_hash(&tx_hash),
                )
            }
            StorageReaderRequest::GetTransactionOutputLocation(transaction_index) => {
                StorageReaderResponse::GetTransactionOutputLocation(
                    txn.get_transaction_output_location(transaction_index),
                )
            }
            StorageReaderRequest::GetTransactionOutputFromFile(location) => {
                StorageReaderResponse::GetTransactionOutputFromFile(
                    txn.get_transaction_output_from_file(location),
                )
            }

            StorageReaderRequest::GetMarker(marker_kind) => {
                StorageReaderResponse::GetMarker(txn.get_marker(marker_kind))
            }
        }
    }

    /// Creates an error response for a given request when transaction opening fails.
    fn error_response_for_request(
        &self,
        request: &StorageReaderRequest,
        error: apollo_storage::StorageError,
    ) -> StorageReaderResponse {
        let err: StorageResult<_> = Err(error);
        
        match request {
            StorageReaderRequest::GetStateDiffLocation(_) => {
                StorageReaderResponse::GetStateDiffLocation(err)
            }
            StorageReaderRequest::GetStateDiffFromFile(_) => {
                StorageReaderResponse::GetStateDiffFromFile(err)
            }
            StorageReaderRequest::GetClassLocation(_) => {
                StorageReaderResponse::GetClassLocation(err)
            }
            StorageReaderRequest::GetClassFromFile(_) => {
                StorageReaderResponse::GetClassFromFile(err)
            }
            StorageReaderRequest::GetClassDeclarationBlock(_) => {
                StorageReaderResponse::GetClassDeclarationBlock(err)
            }
            StorageReaderRequest::GetDeprecatedClassData(_) => {
                StorageReaderResponse::GetDeprecatedClassData(err)
            }
            StorageReaderRequest::GetDeprecatedClassFromFile(_) => {
                StorageReaderResponse::GetDeprecatedClassFromFile(err)
            }
            StorageReaderRequest::GetDeprecatedClassDeclarationBlock(_) => {
                StorageReaderResponse::GetDeprecatedClassDeclarationBlock(err)
            }
            StorageReaderRequest::GetCasmLocation(_) => {
                StorageReaderResponse::GetCasmLocation(err)
            }
            StorageReaderRequest::GetCasmFromFile(_) => {
                StorageReaderResponse::GetCasmFromFile(err)
            }
            StorageReaderRequest::GetExecutableClassHash(_) => {
                StorageReaderResponse::GetExecutableClassHash(err)
            }
            StorageReaderRequest::GetDeployedContractClassHash(_, _) => {
                StorageReaderResponse::GetDeployedContractClassHash(err)
            }
            StorageReaderRequest::GetContractStorageValue(_, _, _) => {
                StorageReaderResponse::GetContractStorageValue(err)
            }
            StorageReaderRequest::GetNonceAtBlock(_, _) => {
                StorageReaderResponse::GetNonceAtBlock(err)
            }
            StorageReaderRequest::GetBlockNumberByHash(_) => {
                StorageReaderResponse::GetBlockNumberByHash(err)
            }
            StorageReaderRequest::GetBlockSignatureByNumber(_) => {
                StorageReaderResponse::GetBlockSignatureByNumber(err)
            }
            StorageReaderRequest::GetTransactionLocation(_) => {
                StorageReaderResponse::GetTransactionLocation(err)
            }
            StorageReaderRequest::GetTransactionFromFile(_) => {
                StorageReaderResponse::GetTransactionFromFile(err)
            }
            StorageReaderRequest::GetTransactionIndexByHash(_) => {
                StorageReaderResponse::GetTransactionIndexByHash(err)
            }
            StorageReaderRequest::GetTransactionOutputLocation(_) => {
                StorageReaderResponse::GetTransactionOutputLocation(err)
            }
            StorageReaderRequest::GetTransactionOutputFromFile(_) => {
                StorageReaderResponse::GetTransactionOutputFromFile(err)
            }
            StorageReaderRequest::GetMarker(_) => StorageReaderResponse::GetMarker(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollo_storage::test_utils::get_test_storage;
    use starknet_api::block::BlockNumber;

    #[test]
    fn test_handle_marker_request() {
        let ((reader, _writer), _temp_dir) = get_test_storage();
        let handler = BatcherStorageReaderHandler::new(reader);

        let request = StorageReaderRequest::GetMarker(apollo_storage::MarkerKind::State);
        let response = handler.handle_request(request);

        match response {
            StorageReaderResponse::GetMarker(result) => {
                assert!(result.is_ok(), "Should successfully get marker");
            }
            _ => panic!("Expected GetMarker response"),
        }
    }

    // TODO: Add more comprehensive tests for all request types.
}

