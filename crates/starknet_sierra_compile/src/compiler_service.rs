use starknet_sierra_compile_types::types::{SierraToCasmCompilerInput, SierraToCasmCompilerResult};

use crate::sierra_to_casm_compiler::SierraToCasmCompiler;

pub type RequestId = u64;
pub struct CompilerService {
    sierra_to_casm_compiler: Box<dyn SierraToCasmCompiler>,
    // TODO(Arni): Decide if we want to use pending requests or just process each request as it
    // comes.
    pending_requests: Vec<SierraToCasmCompilerInput>,
}

impl CompilerService {
    pub fn new(
        sierra_to_casm_compiler: Box<dyn SierraToCasmCompiler>,
        inputs: impl IntoIterator<Item = SierraToCasmCompilerInput>,
    ) -> Self {
        Self { sierra_to_casm_compiler, pending_requests: inputs.into_iter().collect() }
    }

    pub fn add_compile_request(
        &mut self,
        input: SierraToCasmCompilerInput,
    ) -> SierraToCasmCompilerResult<()> {
        self.pending_requests.push(input);
        Ok(())
    }

    /// Processes all pending requests.
    pub fn process_requests(&mut self) -> SierraToCasmCompilerResult<()> {
        for SierraToCasmCompilerInput { contract_class, request_id: _request_id } in
            self.pending_requests.iter()
        {
            let _casm_contract_class =
                self.sierra_to_casm_compiler.compile(contract_class.clone())?;
            // TODO: send casm_contract_class to the next stage
        }
        self.pending_requests.clear();
        Ok(())
    }

    /// Processes a single request.
    pub fn process_request(
        &mut self,
        input: SierraToCasmCompilerInput,
    ) -> SierraToCasmCompilerResult<()> {
        let SierraToCasmCompilerInput { contract_class, request_id: _request_id } = input;
        let _casm_contract_class = self.sierra_to_casm_compiler.compile(contract_class)?;
        // TODO: send casm_contract_class to the next stage
        Ok(())
    }
}
