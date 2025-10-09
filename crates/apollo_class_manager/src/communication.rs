use apollo_class_manager_types::{
    ClassManagerRequest,
    ClassManagerRequestLabelValue,
    ClassManagerResponse,
};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_metrics::generate_permutation_labels;
use async_trait::async_trait;
use strum::VariantNames;

use crate::ClassManager;

pub type LocalClassManagerServer =
    ConcurrentLocalComponentServer<ClassManager, ClassManagerRequest, ClassManagerResponse>;
pub type RemoteClassManagerServer =
    RemoteComponentServer<ClassManagerRequest, ClassManagerResponse>;

#[async_trait]
impl ComponentRequestHandler<ClassManagerRequest, ClassManagerResponse> for ClassManager {
    async fn handle_request(&mut self, request: ClassManagerRequest) -> ClassManagerResponse {
        match request {
            ClassManagerRequest::AddClass(class) => {
                ClassManagerResponse::AddClass(self.0.add_class(class).await)
            }
            ClassManagerRequest::AddClassAndExecutableUnsafe(
                class_id,
                class,
                executable_class_hash_v2,
                executable_class,
            ) => ClassManagerResponse::AddClassAndExecutableUnsafe(
                self.0.add_class_and_executable_unsafe(
                    class_id,
                    class,
                    executable_class_hash_v2,
                    executable_class,
                ),
            ),
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
            ClassManagerRequest::GetExecutableClassHashV2(class_id) => {
                let result = self.0.get_executable_class_hash_v2(class_id);
                ClassManagerResponse::GetExecutableClassHashV2(result)
            }
        }
    }
}

generate_permutation_labels! {
    CLASS_MANAGER_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, ClassManagerRequestLabelValue),
}
