use apollo_storage::storage_reader_communication::{StorageReaderRequest, StorageReaderResponse};
use apollo_storage::storage_reader_handler::StorageReaderHandler;
use apollo_storage::storage_reader_server::StorageReaderServerHandler;
use apollo_storage::{StorageError, StorageReader};
use async_trait::async_trait;

pub struct ClassManagerStorageReaderServerHandler;

#[async_trait]
impl StorageReaderServerHandler<StorageReaderRequest, StorageReaderResponse>
    for ClassManagerStorageReaderServerHandler
{
    async fn handle_request(
        storage_reader: &StorageReader,
        request: StorageReaderRequest,
    ) -> Result<StorageReaderResponse, StorageError> {
        // Validate that the request is relevant to ClassManager.
        // ClassManager needs all class-related operations (Sierra, deprecated, CASM) and markers.
        match &request {
            StorageReaderRequest::GetClassLocation(_)
            | StorageReaderRequest::GetClassFromFile(_)
            | StorageReaderRequest::GetClassDeclarationBlock(_)
            | StorageReaderRequest::GetDeprecatedClassData(_)
            | StorageReaderRequest::GetDeprecatedClassFromFile(_)
            | StorageReaderRequest::GetDeprecatedClassDeclarationBlock(_)
            | StorageReaderRequest::GetCasmLocation(_)
            | StorageReaderRequest::GetCasmFromFile(_)
            | StorageReaderRequest::GetExecutableClassHash(_)
            | StorageReaderRequest::GetMarker(_) => {
                // Request is valid for ClassManager, delegate to unified handler
                let handler = StorageReaderHandler::new(storage_reader.clone());
                handler.handle_request(request)
            }
            _ => Err(StorageError::InvalidRequest {
                component: "ClassManager".to_string(),
                request_type: format!("{:?}", request),
            }),
        }
    }
}
