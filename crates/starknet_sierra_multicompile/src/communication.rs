use async_trait::async_trait;
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use starknet_sequencer_infra::component_server::{
    ConcurrentLocalComponentServer,
    RemoteComponentServer,
};
use starknet_sierra_multicompile_types::{
    SierraCompilerError,
    SierraCompilerRequest,
    SierraCompilerResponse,
};

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
