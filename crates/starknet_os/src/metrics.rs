use blockifier::execution::syscalls::vm_syscall_utils::SyscallUsageMap;
use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::errors::runner_errors::RunnerError;
use cairo_vm::vm::runners::cairo_runner::{CairoRunner, ExecutionResources};
use serde::Serialize;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;

#[derive(Debug, Serialize)]
pub struct OsRunInfo {
    pub pc: MaybeRelocatable,
    pub ap: MaybeRelocatable,
    pub fp: MaybeRelocatable,
    pub used_memory_cells: usize,
}

impl OsRunInfo {
    pub fn new(runner: &mut CairoRunner) -> Self {
        Self {
            pc: runner.vm.get_pc().into(),
            ap: runner.vm.get_ap().into(),
            fp: runner.vm.get_fp().into(),
            used_memory_cells: runner.vm.segments.compute_effective_sizes().iter().sum(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OsMetrics {
    pub syscall_usages: Vec<SyscallUsageMap>,
    pub deprecated_syscall_usages: Vec<SyscallUsageMap>,
    pub run_info: OsRunInfo,
    pub execution_resources: ExecutionResources,
}

impl OsMetrics {
    pub fn new<S: StateReader>(
        runner: &mut CairoRunner,
        hint_processor: &SnosHintProcessor<'_, S>,
    ) -> Result<Self, RunnerError> {
        Ok(Self {
            syscall_usages: hint_processor.execution_helpers_manager.get_syscall_usages(),
            deprecated_syscall_usages: hint_processor
                .execution_helpers_manager
                .get_deprecated_syscall_usages(),
            run_info: OsRunInfo::new(runner),
            execution_resources: runner.get_execution_resources()?,
        })
    }
}
