use async_trait::async_trait;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_class_manager_types::{
    Class,
    ClassHashes,
    ClassManagerRequest,
    ClassManagerResponse,
    ClassManagerResult,
    ExecutableClass,
};
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use starknet_sequencer_infra::component_server::{LocalComponentServer, RemoteComponentServer};

use crate::ClassManager;

pub type LocalClassManagerServer =
    LocalComponentServer<ClassManager, ClassManagerRequest, ClassManagerResponse>;
pub type RemoteClassManagerServer =
    RemoteComponentServer<ClassManagerRequest, ClassManagerResponse>;

// TODO(Elin): rewrite as needed.
#[async_trait]
impl ComponentRequestHandler<ClassManagerRequest, ClassManagerResponse> for ClassManager {
    async fn handle_request(&mut self, request: ClassManagerRequest) -> ClassManagerResponse {
        match request {
            ClassManagerRequest::AddClass(class) => {
                ClassManagerResponse::AddClass(mock_add_class(class))
            }
            ClassManagerRequest::AddDeprecatedClass(class_id, class) => {
                ClassManagerResponse::AddDeprecatedClass(mock_add_deprecated_class(class_id, class))
            }
            ClassManagerRequest::GetExecutable(class_id) => {
                ClassManagerResponse::GetExecutable(mock_get_executable(class_id))
            }
            ClassManagerRequest::GetSierra(class_id) => {
                ClassManagerResponse::GetSierra(mock_get_sierra(class_id))
            }
        }
    }
}

fn mock_add_class(_class: Class) -> ClassManagerResult<ClassHashes> {
    unimplemented!()
}

fn mock_add_deprecated_class(
    _class_id: ClassHash,
    _class: ContractClass,
) -> ClassManagerResult<()> {
    unimplemented!()
}

fn mock_get_executable(_class_id: ClassHash) -> ClassManagerResult<ExecutableClass> {
    unimplemented!()
}

fn mock_get_sierra(_class_id: ClassHash) -> ClassManagerResult<Class> {
    unimplemented!()
}
