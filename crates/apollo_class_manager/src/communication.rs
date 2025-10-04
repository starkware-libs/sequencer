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

// TODO(Elin): change the request and response the server sees to raw types; remove conversions and
// unwraps.
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
                executable_class_id,
                executable_class,
            ) => ClassManagerResponse::AddClassAndExecutableUnsafe(
                self.0.add_class_and_executable_unsafe(
                    class_id,
                    class,
                    executable_class_id,
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
            ClassManagerRequest::GetCasmV1(class_id) => {
                ClassManagerResponse::GetCasmV1(self.0.get_casm_v1(class_id))
            }
            ClassManagerRequest::GetDeprecatedExecutable(class_id) => {
                ClassManagerResponse::GetDeprecatedExecutable(
                    self.0.get_deprecated_executable(class_id),
                )
            }
            ClassManagerRequest::GetSierra(class_id) => {
                ClassManagerResponse::GetSierra(self.0.get_sierra(class_id))
            }
        }
    }
}

generate_permutation_labels! {
    CLASS_MANAGER_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, ClassManagerRequestLabelValue),
}
