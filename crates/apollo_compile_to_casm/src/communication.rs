use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use apollo_compile_to_casm_types::{
    SierraCompilerError,
    SierraCompilerRequest,
    SierraCompilerResponse,
};
use async_trait::async_trait;

use crate::SierraCompiler;

pub type LocalSierraCompilerServer =
    ConcurrentLocalComponentServer<SierraCompiler, SierraCompilerRequest, SierraCompilerResponse>;
pub type RemoteSierraCompilerServer =
    RemoteComponentServer<SierraCompilerRequest, SierraCompilerResponse>;

#[async_trait]
impl ComponentRequestHandler<SierraCompilerRequest, SierraCompilerResponse> for SierraCompiler {
    async fn handle_request(&mut self, request: SierraCompilerRequest) -> SierraCompilerResponse {
        match request {
            SierraCompilerRequest::Compile(contract_class) => {
                let compilation_result =
                    self.compile(contract_class).map_err(SierraCompilerError::from);
                SierraCompilerResponse::Compile(compilation_result)
            }
        }
    }
}
