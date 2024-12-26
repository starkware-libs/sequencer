use async_trait::async_trait;
use starknet_class_manager_types::{ClassManagerRequest, ClassManagerResponse};
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use tracing::instrument;

use crate::{ClassManager, ClassStorage};

#[async_trait]
impl<S: ClassStorage> ComponentRequestHandler<ClassManagerRequest, ClassManagerResponse>
    for ClassManager<S>
{
    #[instrument(skip(self))]
    async fn handle_request(&mut self, request: ClassManagerRequest) -> ClassManagerResponse {
        match request {
            ClassManagerRequest::AddClass(class_id, class) => {
                ClassManagerResponse::AddClass(self.add_class(class_id, class).await)
            }
            ClassManagerRequest::GetExecutable(class_id) => {
                ClassManagerResponse::GetExecutable(self.get_executable(class_id))
            }
            ClassManagerRequest::GetSierra(class_id) => {
                ClassManagerResponse::GetSierra(self.get_sierra(class_id))
            }
            ClassManagerRequest::AddDeprecatedClass(class_id, class) => {
                ClassManagerResponse::AddDeprecatedClass(self.add_deprecated_class(class_id, class))
            }
        }
    }
}
