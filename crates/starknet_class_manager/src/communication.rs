use async_trait::async_trait;
use starknet_class_manager_types::{ClassManagerRequest, ClassManagerResponse};
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use starknet_sequencer_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use starknet_sierra_multicompile_types::RawExecutableClass;

use crate::ClassManager;

pub type LocalClassManagerServer =
    LocalComponentServer<ClassManager, ClassManagerRequest, ClassManagerResponse>;
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
            ClassManagerRequest::AddDeprecatedClass(class_id, class) => {
                let x = starknet_api::contract_class::ContractClass::V0(class);
                let c = RawExecutableClass::try_from(x).unwrap();
                println!("******** c: {:?}", c);
                ClassManagerResponse::AddDeprecatedClass(
                    // self.0.add_deprecated_class(class_id, x.try_into().unwrap()),
                    self.0.add_deprecated_class(class_id, c),
                )
            }
            ClassManagerRequest::GetExecutable(class_id) => {
                let result = self.0.get_executable(class_id).map(|optional_class| {
                    optional_class.map(|class| {
                        let copy = class.clone();
                        starknet_api::contract_class::ContractClass::try_from(class).unwrap_or_else(
                            |e| panic!("Failed to convert class with err {e}. class: {:?}", copy),
                        )
                        // class.try_into().unwrap_or_else(|e| {
                        //     panic!("Failed to convert class with err {e}. class: {:?}", copy)
                        // })
                    })
                });
                ClassManagerResponse::GetExecutable(result)
            }
            ClassManagerRequest::GetSierra(class_id) => {
                let result = self
                    .0
                    .get_sierra(class_id)
                    .map(|optional_class| optional_class.map(|class| class.try_into().unwrap()));
                ClassManagerResponse::GetSierra(result)
            }
        }
    }
}
