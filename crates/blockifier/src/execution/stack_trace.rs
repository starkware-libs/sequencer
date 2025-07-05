use std::fmt::{Display, Formatter};
use std::sync::LazyLock;

use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use itertools::Itertools;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use starknet_api::execution_utils::format_panic_data;
use starknet_types_core::felt::Felt;

use crate::execution::call_info::{CallInfo, Retdata};
use crate::execution::deprecated_syscalls::hint_processor::DeprecatedSyscallExecutionError;
use crate::execution::errors::{ConstructorEntryPointExecutionError, EntryPointExecutionError};
use crate::execution::syscalls::hint_processor::{SyscallExecutionError, ENTRYPOINT_FAILED_ERROR};
use crate::transaction::errors::TransactionExecutionError;

#[cfg(test)]
#[path = "stack_trace_test.rs"]
pub mod test;

pub const TRACE_LENGTH_CAP: usize = 15000;
pub const TRACE_EXTRA_CHARS_SLACK: usize = 100;

#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub enum PreambleType {
    CallContract,
    LibraryCall,
    Constructor,
}

impl PreambleType {
    pub fn text(&self) -> &str {
        match self {
            Self::CallContract => "Error in the called contract",
            Self::LibraryCall => "Error in a library call",
            Self::Constructor => "Error in the contract class constructor",
        }
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialEq)]
pub struct EntryPointErrorFrame {
    pub depth: usize,
    pub preamble_type: PreambleType,
    pub storage_address: ContractAddress,
    pub class_hash: ClassHash,
    pub selector: Option<EntryPointSelector>,
}

impl EntryPointErrorFrame {
    fn preamble_text(&self) -> String {
        format!(
            "{}: {} (contract address: {:#064x}, class hash: {:#064x}, selector: {}):",
            self.depth,
            self.preamble_type.text(),
            self.storage_address.0.key(),
            self.class_hash.0,
            if let Some(selector) = self.selector {
                format!("{:#064x}", selector.0)
            } else {
                "UNKNOWN".to_string()
            }
        )
    }
}

impl From<&EntryPointErrorFrame> for String {
    fn from(value: &EntryPointErrorFrame) -> Self {
        value.preamble_text()
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialEq)]
pub struct VmExceptionFrame {
    pub pc: Relocatable,
    pub error_attr_value: Option<String>,
    pub traceback: Option<String>,
}

impl From<&VmExceptionFrame> for String {
    fn from(value: &VmExceptionFrame) -> Self {
        let error_msg = match &value.error_attr_value {
            Some(error_msg) => error_msg.clone(),
            None => String::new(),
        };
        let vm_exception_preamble = format!("Error at pc={}:", value.pc);
        let vm_exception_traceback = if let Some(traceback) = &value.traceback {
            format!("\n{traceback}")
        } else {
            "".to_string()
        };
        format!("{error_msg}{vm_exception_preamble}{vm_exception_traceback}")
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialEq, derive_more::From)]
pub enum ErrorStackSegment {
    EntryPoint(EntryPointErrorFrame),
    Cairo1RevertSummary(Cairo1RevertSummary),
    Vm(VmExceptionFrame),
    StringFrame(String),
}

impl From<&ErrorStackSegment> for String {
    fn from(value: &ErrorStackSegment) -> Self {
        match value {
            ErrorStackSegment::EntryPoint(entry_point_frame) => entry_point_frame.into(),
            ErrorStackSegment::Cairo1RevertSummary(cairo1_revert_stack) => {
                cairo1_revert_stack.to_string()
            }
            ErrorStackSegment::Vm(vm_exception_frame) => vm_exception_frame.into(),
            ErrorStackSegment::StringFrame(error) => error.clone(),
        }
    }
}

#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ErrorStackHeader {
    Constructor,
    Execution,
    Validation,
    #[default]
    None,
}

impl Display for ErrorStackHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Constructor => "Contract constructor execution has failed:\n",
                Self::Execution => "Transaction execution has failed:\n",
                Self::Validation => "Transaction validation has failed:\n",
                Self::None => "",
            }
        )
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, PartialEq)]
pub struct ErrorStack {
    pub header: ErrorStackHeader,
    pub stack: Vec<ErrorStackSegment>,
}

impl Display for ErrorStack {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let error_stack_str = self.stack.iter().map(String::from).join("\n");

        // When the trace string is too long, trim it in a way that keeps both the beginning and
        // end.
        let final_str = if error_stack_str.len() > TRACE_LENGTH_CAP + TRACE_EXTRA_CHARS_SLACK {
            error_stack_str[..(TRACE_LENGTH_CAP / 2)].to_string()
                + "\n\n...\n\n"
                + &error_stack_str[(error_stack_str.len() - TRACE_LENGTH_CAP / 2)..]
        } else {
            error_stack_str
        };
        write!(f, "{}{}", self.header, final_str)
    }
}

impl ErrorStack {
    pub fn push(&mut self, frame: ErrorStackSegment) {
        self.stack.push(frame);
    }
}

#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct Cairo1RevertFrame {
    pub contract_address: ContractAddress,
    pub class_hash: Option<ClassHash>,
    pub selector: EntryPointSelector,
}

pub static MIN_CAIRO1_FRAME_LENGTH: LazyLock<usize> = LazyLock::new(|| {
    let frame = Cairo1RevertFrame {
        contract_address: ContractAddress::default(),
        class_hash: Some(ClassHash::default()),
        selector: EntryPointSelector::default(),
    };
    // +1 for newline.
    format!("{frame}").len() + 1
});

impl From<&&CallInfo> for Cairo1RevertFrame {
    fn from(callinfo: &&CallInfo) -> Self {
        Self {
            contract_address: callinfo.call.storage_address,
            class_hash: callinfo.call.class_hash,
            selector: callinfo.call.entry_point_selector,
        }
    }
}

impl Display for Cairo1RevertFrame {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error in contract (contract address: {:#064x}, class hash: {}, selector: {:#064x}):",
            self.contract_address.0.key(),
            match self.class_hash {
                Some(class_hash) => format!("{:#064x}", class_hash.0),
                None => "_".to_string(),
            },
            self.selector.0,
        )
    }
}

#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cairo1RevertHeader {
    Execution,
    Validation,
}

impl Display for Cairo1RevertHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Execution => "Execution failed. Failure reason:",
                Self::Validation => "The `validate` entry point panicked with:",
            }
        )
    }
}

#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct Cairo1RevertSummary {
    pub header: Cairo1RevertHeader,
    pub stack: Vec<Cairo1RevertFrame>,
    pub last_retdata: Retdata,
}

impl Cairo1RevertSummary {
    pub const TRUNCATION_SEPARATOR: &'static str = "\n...";
}

pub static MIN_CAIRO1_FRAMES_STACK_LENGTH: LazyLock<usize> = LazyLock::new(|| {
    // Two frames (first and last) + separator.
    2 * *MIN_CAIRO1_FRAME_LENGTH + Cairo1RevertSummary::TRUNCATION_SEPARATOR.len()
});

impl Display for Cairo1RevertSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Total string length is limited by TRACE_LENGTH_CAP.

        let header = format!("{}", self.header);
        let tail = ".\n";

        // Prioritize the failure reason felts over the frames.
        // If the failure reason is too long to include a minimal frame trace + header + newline,
        // display only the failure reason (truncated if necessary).
        let failure_reason = format_panic_data(&self.last_retdata.0);
        let string_without_frames =
            [header.clone(), failure_reason.clone(), tail.into()].join("\n");
        if string_without_frames.len() >= TRACE_LENGTH_CAP - *MIN_CAIRO1_FRAMES_STACK_LENGTH - 1 {
            let output = if string_without_frames.len() <= TRACE_LENGTH_CAP {
                string_without_frames
            } else {
                string_without_frames
                    .chars()
                    .take(TRACE_LENGTH_CAP - Self::TRUNCATION_SEPARATOR.len())
                    .collect::<String>()
                    + Self::TRUNCATION_SEPARATOR
            };
            return write!(f, "{output}");
        }

        let untruncated_string = [header.clone()]
            .into_iter()
            .chain(self.stack.iter().map(|frame| frame.to_string()))
            .chain([failure_reason.clone()])
            .join("\n")
            + tail;
        if untruncated_string.len() <= TRACE_LENGTH_CAP {
            return write!(f, "{untruncated_string}");
        }

        // If the number of frames is too large, drop frames above the last frame (two frames are
        // not too many, as checked above with MIN_CAIRO1_FRAMES_STACK_LENGTH).
        let n_frames_to_drop = (untruncated_string.len() - TRACE_LENGTH_CAP
            + Self::TRUNCATION_SEPARATOR.len())
        .div_ceil(*MIN_CAIRO1_FRAME_LENGTH);

        // If the number of frames is not as expected, fall back to the failure reason.
        let final_string =
            match (self.stack.get(..self.stack.len() - n_frames_to_drop - 1), self.stack.last()) {
                (Some(frames), Some(last_frame)) => {
                    let combined_string = [header]
                        .into_iter()
                        .chain(frames.iter().map(|frame| frame.to_string()))
                        .chain([
                            String::from(Self::TRUNCATION_SEPARATOR),
                            last_frame.to_string(),
                            failure_reason,
                        ])
                        .join("\n")
                        + tail;
                    if combined_string.len() <= TRACE_LENGTH_CAP {
                        combined_string
                    } else {
                        // If the combined string is too long, truncate it.
                        combined_string
                            .chars()
                            .take(TRACE_LENGTH_CAP - Self::TRUNCATION_SEPARATOR.len())
                            .collect::<String>()
                            + Self::TRUNCATION_SEPARATOR
                    }
                }
                _ => failure_reason,
            };
        write!(
            f,
            "{}",
            // Truncate again as a failsafe.
            final_string.chars().take(TRACE_LENGTH_CAP).collect::<String>()
        )
    }
}

pub fn extract_trailing_cairo1_revert_trace(
    root_call: &CallInfo,
    header: Cairo1RevertHeader,
) -> Cairo1RevertSummary {
    let fallback_value = Cairo1RevertSummary {
        header,
        stack: vec![],
        last_retdata: root_call.execution.retdata.clone(),
    };
    let entrypoint_failed_felt = Felt::from_hex(ENTRYPOINT_FAILED_ERROR)
        .unwrap_or_else(|_| panic!("{ENTRYPOINT_FAILED_ERROR} does not fit in a felt."));

    // Compute the failing call chain.
    let mut error_calls: Vec<&CallInfo> = vec![];
    let mut call = root_call;
    // It is possible that a failing contract managed to call another (non-failing) contract
    // before hitting an error; stop iteration if the current call was successful.
    while call.execution.failed {
        error_calls.push(call);
        // If the last felt in the retdata is not the failure felt, stop iteration.
        // Even if the next inner call is also in failed state, assume a scenario where the current
        // call panicked after ignoring the error result of the inner call.
        let retdata = &call.execution.retdata.0;
        if retdata.last() != Some(&entrypoint_failed_felt) {
            break;
        }
        // Select the next inner failure, if it exists and is unique.
        // Consider the following scenario:
        // ```
        // let A = call_contract(...)
        // let B = call_contract(...)
        // X.unwrap_syscall(...)
        // ```
        // where X is either A or B. If both A and B are calls to different contracts, which fail
        // for different reasons but return the same failure reasons, we cannot distinguish between
        // them - i.e. we cannot distinguish between the case X=A or X=B.
        // To avoid returning misleading data, we revert to the fallback value in such cases.
        // If the source of failure can be identified in the inner calls, iterate.
        let expected_inner_retdata = &retdata[..(retdata.len() - 1)];
        let mut potential_inner_failures_iter = call.inner_calls.iter().filter(|inner_call| {
            inner_call.execution.failed
                && &inner_call.execution.retdata.0[..] == expected_inner_retdata
        });
        call = match potential_inner_failures_iter.next() {
            Some(unique_inner_failure) if potential_inner_failures_iter.next().is_none() => {
                unique_inner_failure
            }
            // Inner failure is either not unique, or does not exist (malformed retdata).
            _ => return fallback_value,
        };
    }

    // Add one line per call, and append the failure reason.
    // If error_calls is empty, that means the root call is non-failing; return the fallback value.
    let Some(last_call) = error_calls.last() else { return fallback_value };
    Cairo1RevertSummary {
        header,
        stack: error_calls.iter().map(Cairo1RevertFrame::from).collect(),
        last_retdata: last_call.execution.retdata.clone(),
    }
}

/// Extracts the error trace from a `TransactionExecutionError`. This is a top level function.
pub fn gen_tx_execution_error_trace(error: &TransactionExecutionError) -> ErrorStack {
    match error {
        TransactionExecutionError::ExecutionError {
            error,
            class_hash,
            storage_address,
            selector,
        } => gen_error_trace_from_entry_point_error(
            ErrorStackHeader::Execution,
            error,
            storage_address,
            class_hash,
            Some(selector),
            PreambleType::CallContract,
        ),
        TransactionExecutionError::ValidateTransactionError {
            error,
            class_hash,
            storage_address,
            selector,
        } => gen_error_trace_from_entry_point_error(
            ErrorStackHeader::Validation,
            error,
            storage_address,
            class_hash,
            Some(selector),
            PreambleType::CallContract,
        ),
        TransactionExecutionError::ContractConstructorExecutionFailed(
            ConstructorEntryPointExecutionError::ExecutionError {
                error,
                class_hash,
                contract_address: storage_address,
                constructor_selector,
            },
        ) => gen_error_trace_from_entry_point_error(
            ErrorStackHeader::Constructor,
            error,
            storage_address,
            class_hash,
            constructor_selector.as_ref(),
            PreambleType::Constructor,
        ),
        TransactionExecutionError::PanicInValidate { panic_reason } => {
            let mut stack = ErrorStack::default();
            stack.push(panic_reason.clone().into());
            stack
        }
        _ => {
            // Top-level error is unrelated to Cairo execution, no "real" frames.
            let mut stack = ErrorStack::default();
            stack.push(ErrorStackSegment::StringFrame(error.to_string()));
            stack
        }
    }
}

/// Generate error stack from top-level entry point execution error.
fn gen_error_trace_from_entry_point_error(
    header: ErrorStackHeader,
    error: &EntryPointExecutionError,
    storage_address: &ContractAddress,
    class_hash: &ClassHash,
    entry_point_selector: Option<&EntryPointSelector>,
    preamble_type: PreambleType,
) -> ErrorStack {
    let mut error_stack = ErrorStack { header, ..Default::default() };
    let depth = 0;
    error_stack.push(
        EntryPointErrorFrame {
            depth,
            preamble_type,
            storage_address: *storage_address,
            class_hash: *class_hash,
            selector: entry_point_selector.copied(),
        }
        .into(),
    );
    extract_entry_point_execution_error_into_stack_trace(&mut error_stack, depth + 1, error);
    error_stack
}

fn extract_cairo_run_error_into_stack_trace(
    error_stack: &mut ErrorStack,
    depth: usize,
    error: &CairoRunError,
) {
    if let CairoRunError::VmException(vm_exception) = error {
        error_stack.push(
            VmExceptionFrame {
                pc: vm_exception.pc,
                error_attr_value: vm_exception.error_attr_value.clone(),
                traceback: vm_exception.traceback.clone(),
            }
            .into(),
        );
        extract_virtual_machine_error_into_stack_trace(error_stack, depth, &vm_exception.inner_exc);
    } else {
        error_stack.push(error.to_string().into());
    }
}

fn extract_virtual_machine_error_into_stack_trace(
    error_stack: &mut ErrorStack,
    depth: usize,
    vm_error: &VirtualMachineError,
) {
    match vm_error {
        VirtualMachineError::Hint(ref boxed_hint_error) => {
            if let HintError::Internal(internal_vm_error) = &boxed_hint_error.1 {
                return extract_virtual_machine_error_into_stack_trace(
                    error_stack,
                    depth,
                    internal_vm_error,
                );
            }
            error_stack.push(boxed_hint_error.1.to_string().into());
        }
        VirtualMachineError::Other(anyhow_error) => {
            let syscall_exec_err = anyhow_error.downcast_ref::<SyscallExecutionError>();
            if let Some(downcast_anyhow) = syscall_exec_err {
                extract_syscall_execution_error_into_stack_trace(
                    error_stack,
                    depth,
                    downcast_anyhow,
                )
            } else {
                let deprecated_syscall_exec_err =
                    anyhow_error.downcast_ref::<DeprecatedSyscallExecutionError>();
                if let Some(downcast_anyhow) = deprecated_syscall_exec_err {
                    extract_deprecated_syscall_execution_error_into_stack_trace(
                        error_stack,
                        depth,
                        downcast_anyhow,
                    )
                }
            }
        }
        _ => {
            error_stack.push(format!("{vm_error}\n").into());
        }
    }
}

fn extract_syscall_execution_error_into_stack_trace(
    error_stack: &mut ErrorStack,
    depth: usize,
    syscall_error: &SyscallExecutionError,
) {
    match syscall_error {
        SyscallExecutionError::CallContractExecutionError {
            class_hash,
            storage_address,
            selector,
            error,
        } => {
            error_stack.push(
                EntryPointErrorFrame {
                    depth,
                    preamble_type: PreambleType::CallContract,
                    storage_address: *storage_address,
                    class_hash: *class_hash,
                    selector: Some(*selector),
                }
                .into(),
            );
            extract_syscall_execution_error_into_stack_trace(error_stack, depth + 1, error)
        }
        SyscallExecutionError::LibraryCallExecutionError {
            class_hash,
            storage_address,
            selector,
            error,
        } => {
            error_stack.push(
                EntryPointErrorFrame {
                    depth,
                    preamble_type: PreambleType::LibraryCall,
                    storage_address: *storage_address,
                    class_hash: *class_hash,
                    selector: Some(*selector),
                }
                .into(),
            );
            extract_syscall_execution_error_into_stack_trace(error_stack, depth + 1, error);
        }
        SyscallExecutionError::ConstructorEntryPointExecutionError(
            ConstructorEntryPointExecutionError::ExecutionError {
                error,
                class_hash,
                contract_address,
                constructor_selector,
            },
        ) => {
            error_stack.push(
                EntryPointErrorFrame {
                    depth,
                    preamble_type: PreambleType::Constructor,
                    storage_address: *contract_address,
                    class_hash: *class_hash,
                    selector: *constructor_selector,
                }
                .into(),
            );
            extract_entry_point_execution_error_into_stack_trace(error_stack, depth, error)
        }
        SyscallExecutionError::EntryPointExecutionError(entry_point_error) => {
            extract_entry_point_execution_error_into_stack_trace(
                error_stack,
                depth,
                entry_point_error,
            )
        }
        _ => {
            error_stack.push(syscall_error.to_string().into());
        }
    }
}

fn extract_deprecated_syscall_execution_error_into_stack_trace(
    error_stack: &mut ErrorStack,
    depth: usize,
    syscall_error: &DeprecatedSyscallExecutionError,
) {
    match syscall_error {
        DeprecatedSyscallExecutionError::CallContractExecutionError {
            class_hash,
            storage_address,
            selector,
            error,
        } => {
            error_stack.push(
                EntryPointErrorFrame {
                    depth,
                    preamble_type: PreambleType::CallContract,
                    storage_address: *storage_address,
                    class_hash: *class_hash,
                    selector: Some(*selector),
                }
                .into(),
            );
            extract_deprecated_syscall_execution_error_into_stack_trace(
                error_stack,
                depth + 1,
                error,
            )
        }
        DeprecatedSyscallExecutionError::LibraryCallExecutionError {
            class_hash,
            storage_address,
            selector,
            error,
        } => {
            error_stack.push(
                EntryPointErrorFrame {
                    depth,
                    preamble_type: PreambleType::LibraryCall,
                    storage_address: *storage_address,
                    class_hash: *class_hash,
                    selector: Some(*selector),
                }
                .into(),
            );
            extract_deprecated_syscall_execution_error_into_stack_trace(
                error_stack,
                depth + 1,
                error,
            )
        }
        DeprecatedSyscallExecutionError::ConstructorEntryPointExecutionError(
            ConstructorEntryPointExecutionError::ExecutionError {
                error,
                class_hash,
                contract_address,
                constructor_selector,
            },
        ) => {
            error_stack.push(
                EntryPointErrorFrame {
                    depth,
                    preamble_type: PreambleType::Constructor,
                    storage_address: *contract_address,
                    class_hash: *class_hash,
                    selector: *constructor_selector,
                }
                .into(),
            );
            extract_entry_point_execution_error_into_stack_trace(error_stack, depth, error)
        }
        DeprecatedSyscallExecutionError::EntryPointExecutionError(entry_point_error) => {
            extract_entry_point_execution_error_into_stack_trace(
                error_stack,
                depth,
                entry_point_error,
            )
        }
        _ => error_stack.push(syscall_error.to_string().into()),
    }
}

fn extract_entry_point_execution_error_into_stack_trace(
    error_stack: &mut ErrorStack,
    depth: usize,
    entry_point_error: &EntryPointExecutionError,
) {
    match entry_point_error {
        EntryPointExecutionError::CairoRunError(cairo_run_error) => {
            extract_cairo_run_error_into_stack_trace(error_stack, depth, cairo_run_error)
        }
        #[cfg(feature = "cairo_native")]
        EntryPointExecutionError::NativeUnrecoverableError(error) => {
            extract_syscall_execution_error_into_stack_trace(error_stack, depth, error)
        }
        EntryPointExecutionError::ExecutionFailed { error_trace } => {
            error_stack.push(error_trace.clone().into())
        }
        _ => error_stack.push(format!("{entry_point_error}\n").into()),
    }
}
