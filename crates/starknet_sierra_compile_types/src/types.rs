use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use serde::{Deserialize, Serialize};

use crate::errors::StarknetSierraCompilerServiceError;

// TODO(Arni): Placeholder request id. Replace with a more meaningful type.
pub type RequestId = u64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SierraToCasmCompilerInput {
    pub contract_class: ContractClass,
    pub request_id: RequestId,
}

pub struct SierraToCasmCompilerOutput {
    pub casm_contract_class: CasmContractClass,
}

pub type SierraToCasmCompilerResult<T> = Result<T, StarknetSierraCompilerServiceError>;
