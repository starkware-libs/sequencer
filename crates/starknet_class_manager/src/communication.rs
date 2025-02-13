use async_trait::async_trait;
use starknet_class_manager_types::{ClassManagerRequest, ClassManagerResponse};
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use starknet_sequencer_infra::component_server::{LocalComponentServer, RemoteComponentServer};

use crate::ClassManager;

pub type LocalClassManagerServer =
    LocalComponentServer<ClassManager, ClassManagerRequest, ClassManagerResponse>;
pub type RemoteClassManagerServer =
    RemoteComponentServer<ClassManagerRequest, ClassManagerResponse>;

#[async_trait]
impl ComponentRequestHandler<ClassManagerRequest, ClassManagerResponse> for ClassManager {
    async fn handle_request(&mut self, request: ClassManagerRequest) -> ClassManagerResponse {
        match request {
            ClassManagerRequest::AddClass(class) => {
                ClassManagerResponse::AddClass(self.0.add_class(class).await)
            }
            ClassManagerRequest::AddDeprecatedClass(class_id, class) => {
                ClassManagerResponse::AddDeprecatedClass(
                    self.0.add_deprecated_class(class_id, class),
                )
            }
            ClassManagerRequest::GetExecutable(class_id) => {
                ClassManagerResponse::GetExecutable(self.0.get_executable(class_id))
            }
            ClassManagerRequest::GetSierra(class_id) => {
                ClassManagerResponse::GetSierra(self.0.get_sierra(class_id))
            }
        }
    }
}
