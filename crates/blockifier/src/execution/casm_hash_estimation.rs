use cairo_vm::vm::runners::cairo_runner::ExecutionResources;

/// Represents an estimate of VM resources used by Cairo functions in the Starknet OS.
#[derive(Debug)]
pub enum EstimatedExecutionResources {
    V1 { resources: ExecutionResources },
    V2 { resources: ExecutionResources, blake_count: usize },
}
