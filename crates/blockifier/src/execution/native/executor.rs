use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::executor::AotContractExecutor;
use cairo_native::utils::BuiltinCosts;
use starknet_types_core::felt::Felt;

use crate::execution::native::syscall_handler::NativeSyscallHandler;

#[derive(Debug)]
pub enum ContractExecutor {
    Aot(AotContractExecutor),
}

impl From<AotContractExecutor> for ContractExecutor {
    fn from(value: AotContractExecutor) -> Self {
        Self::Aot(value)
    }
}

impl ContractExecutor {
    pub fn run(
        &self,
        selector: Felt,
        args: &[Felt],
        gas: u64,
        builtin_costs: Option<BuiltinCosts>,
        syscall_handler: &mut NativeSyscallHandler<'_>,
    ) -> cairo_native::error::Result<ContractExecutionResult> {
        match self {
            ContractExecutor::Aot(aot_contract_executor) => {
                aot_contract_executor.run(selector, args, gas, builtin_costs, syscall_handler)
            }
        }
    }
}
