use apollo_class_manager_types::{ClassManagerRequest, ClassManagerResponse};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;
use starknet_api::contract_class::ContractClass;

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
                ClassManagerResponse::AddClass(self.0.add_class(class.try_into().unwrap()).await)
            }
            ClassManagerRequest::AddClassAndExecutableUnsafe(
                class_id,
                class,
                executable_class_hash_v2,
                executable_class,
            ) => ClassManagerResponse::AddClassAndExecutableUnsafe(
                self.0.add_class_and_executable_unsafe(
                    class_id,
                    class.try_into().unwrap(),
                    executable_class_hash_v2,
                    executable_class.try_into().unwrap(),
                ),
            ),
            ClassManagerRequest::AddDeprecatedClass(class_id, class) => {
                let class = ContractClass::V0(class).try_into().unwrap();
                ClassManagerResponse::AddDeprecatedClass(
                    self.0.add_deprecated_class(class_id, class),
                )
            }
            ClassManagerRequest::GetExecutable(class_id) => {
                let result = self
                    .0
                    .get_executable(class_id)
                    .map(|optional_class| optional_class.map(|class| class.try_into().unwrap()));
                ClassManagerResponse::GetExecutable(result)
            }
            ClassManagerRequest::GetSierra(class_id) => {
                let result = self
                    .0
                    .get_sierra(class_id)
                    .map(|optional_class| optional_class.map(|class| class.try_into().unwrap()));
                ClassManagerResponse::GetSierra(result)
            }
            ClassManagerRequest::GetExecutableClassHashV2(class_id) => {
                let result = self.0.get_executable_class_hash_v2(class_id);
                ClassManagerResponse::GetExecutableClassHashV2(result)
            }
        }
    }
}
