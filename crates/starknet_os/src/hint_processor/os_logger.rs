use std::collections::HashMap;

use blockifier::execution::syscalls::SyscallSelector;
use blockifier::transaction::transaction_types::TransactionType;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::transaction::TransactionHash;

#[derive(Debug, thiserror::Error)]
pub enum OsLoggerError {
    #[error("SyscallTrace should be finalized only once.")]
    DoubleFinalize,
    #[error("SyscallTrace should be finalized before accessing resources.")]
    ResourceAccessBeforeFinalize,
}

pub type OsLoggerResult<T> = Result<T, OsLoggerError>;

pub trait ResourceFinalizer {
    fn get_optional_resources(&self) -> Option<&ExecutionResources>;

    fn set_resources(&mut self, resources: ExecutionResources);

    fn get_resources(&self) -> OsLoggerResult<&ExecutionResources> {
        self.get_optional_resources().ok_or(OsLoggerError::ResourceAccessBeforeFinalize)
    }

    fn finalize_resources(&mut self, resources: ExecutionResources) -> OsLoggerResult<()> {
        if self.get_optional_resources().is_some() {
            return Err(OsLoggerError::DoubleFinalize);
        }
        self.set_resources(resources);
        Ok(())
    }
}

pub struct SyscallTrace {
    selector: SyscallSelector,
    is_deprecated: bool,
    tab_count: usize,
    inner_syscalls: Vec<SyscallTrace>,
    resources: Option<ExecutionResources>,
}

impl SyscallTrace {
    pub fn new(selector: SyscallSelector, is_deprecated: bool, tab_count: usize) -> Self {
        Self { selector, is_deprecated, tab_count, inner_syscalls: Vec::new(), resources: None }
    }
}

impl ResourceFinalizer for SyscallTrace {
    fn get_optional_resources(&self) -> Option<&ExecutionResources> {
        self.resources.as_ref()
    }

    fn set_resources(&mut self, resources: ExecutionResources) {
        self.resources = Some(resources);
    }
}

impl TryFrom<SyscallTrace> for String {
    type Error = OsLoggerError;

    fn try_from(trace: SyscallTrace) -> OsLoggerResult<Self> {
        let deprecated_prefix = if trace.is_deprecated { "deprecated " } else { "" };
        let indentation = "  ".repeat(trace.tab_count + 1);
        let resources = trace.get_resources()?;

        let builtins = if !resources.builtin_instance_counter.is_empty() {
            format!("\n{indentation}Builtins: {:?}", resources.builtin_instance_counter)
        } else {
            "".to_string()
        };

        let inner_syscalls = if !trace.inner_syscalls.is_empty() {
            // Count inner syscalls.
            let mut syscall_count: HashMap<SyscallSelector, usize> = HashMap::new();
            for inner_syscall in &trace.inner_syscalls {
                *syscall_count.entry(inner_syscall.selector).or_insert(0) += 1;
            }
            format!("\n{indentation}Inner syscalls: {syscall_count:?}")
        } else {
            "".to_string()
        };

        Ok(format!(
            "{deprecated_prefix}Syscall: {:?}\n{indentation}Steps: {}{builtins}{inner_syscalls}",
            trace.selector, resources.n_steps
        ))
    }
}

pub struct OsTransactionTrace {
    tx_type: TransactionType,
    tx_hash: TransactionHash,
    #[allow(dead_code)]
    syscalls: Vec<SyscallTrace>,
    resources: Option<ExecutionResources>,
}

impl OsTransactionTrace {
    pub fn new(tx_type: TransactionType, tx_hash: TransactionHash) -> Self {
        Self { tx_type, tx_hash, syscalls: Vec::new(), resources: None }
    }
}

impl ResourceFinalizer for OsTransactionTrace {
    fn get_optional_resources(&self) -> Option<&ExecutionResources> {
        self.resources.as_ref()
    }

    fn set_resources(&mut self, resources: ExecutionResources) {
        self.resources = Some(resources);
    }
}

impl TryFrom<OsTransactionTrace> for String {
    type Error = OsLoggerError;

    fn try_from(trace: OsTransactionTrace) -> OsLoggerResult<Self> {
        let resources = trace.get_resources()?;
        let builtins = if !resources.builtin_instance_counter.is_empty() {
            format!("\n\tBuiltins: {:?}", resources.builtin_instance_counter)
        } else {
            "".to_string()
        };
        Ok(format!(
            "Transaction: {:?}\n\tHash: {}\n\tSteps: {}{builtins}",
            trace.tx_type, trace.tx_hash, resources.n_steps
        ))
    }
}
