use std::collections::HashMap;

use blockifier::execution::syscalls::SyscallSelector;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;

#[derive(Debug, thiserror::Error)]
pub enum OsLoggerError {
    #[error("SyscallTrace should be finalized only once.")]
    DoubleFinalize,
    #[error("SyscallTrace should be finalized before accessing resources.")]
    ResourceAccessBeforeFinalize,
}

pub type OsLoggerResult<T> = Result<T, OsLoggerError>;

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

    pub fn get_resources(&self) -> OsLoggerResult<&ExecutionResources> {
        self.resources.as_ref().ok_or(OsLoggerError::ResourceAccessBeforeFinalize)
    }

    pub fn finalize_resources(&mut self, resources: ExecutionResources) -> OsLoggerResult<()> {
        if self.resources.is_some() {
            return Err(OsLoggerError::DoubleFinalize);
        }
        self.resources = Some(resources);
        Ok(())
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
