use async_trait::async_trait;
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use starknet_sierra_compile_types::{
    SierraCompilerError,
    SierraCompilerRequest,
    SierraCompilerResponse,
};
use tracing::instrument;

use crate::{SierraCompiler, SierraToCasmCompiler};

#[async_trait]
impl<C: SierraToCasmCompiler> ComponentRequestHandler<SierraCompilerRequest, SierraCompilerResponse>
    for SierraCompiler<C>
{
    #[instrument(skip(self))]
    async fn handle_request(&mut self, request: SierraCompilerRequest) -> SierraCompilerResponse {
        match request {
            SierraCompilerRequest::Compile(class) => {
                let raw_executable_class = self
                    .compile(class)
                    .map_err(|error| SierraCompilerError::SierraCompilerError(error.to_string()));
                SierraCompilerResponse::Compile(raw_executable_class)
            }
        }
    }
}
