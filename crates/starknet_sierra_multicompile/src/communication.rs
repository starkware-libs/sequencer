use async_trait::async_trait;
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use starknet_sequencer_infra::component_server::{
    ConcurrentLocalComponentServer,
    RemoteComponentServer,
};
use starknet_sierra_multicompile_types::{
    RawClass,
    RawExecutableHashedClass,
    SierraCompilerRequest,
    SierraCompilerResponse,
    SierraCompilerResult,
};

use crate::SierraCompiler;

pub type LocalSierraCompilerServer =
    ConcurrentLocalComponentServer<SierraCompiler, SierraCompilerRequest, SierraCompilerResponse>;
pub type RemoteSierraCompilerServer =
    RemoteComponentServer<SierraCompilerRequest, SierraCompilerResponse>;

// TODO(Elin): change this function as needed.
#[async_trait]
impl ComponentRequestHandler<SierraCompilerRequest, SierraCompilerResponse> for SierraCompiler {
    async fn handle_request(&mut self, request: SierraCompilerRequest) -> SierraCompilerResponse {
        match request {
            SierraCompilerRequest::Compile(contract_class) => {
                // Cant use self.compile(..) because of needed consolidation in the
                // SierraCompilerResult.
                SierraCompilerResponse::Compile(compile(contract_class).await)
            }
        }
    }
}

async fn compile(_contract_class: RawClass) -> SierraCompilerResult<RawExecutableHashedClass> {
    unimplemented!()
}
