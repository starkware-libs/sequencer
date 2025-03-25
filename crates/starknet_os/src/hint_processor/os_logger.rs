use std::collections::HashMap;

use blockifier::execution::syscalls::SyscallSelector;
use blockifier::transaction::transaction_types::TransactionType;
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::transaction::TransactionHash;

use crate::hints::error::OsHintError;
use crate::hints::vars::{CairoStruct, Ids};
use crate::vm_utils::get_address_of_nested_fields;

#[derive(Debug, thiserror::Error)]
pub enum OsLoggerError {
    #[error("Failed to build builtin pointer map: {0}.")]
    BuiltinPtrs(OsHintError),
    #[error("SyscallTrace should be finalized only once.")]
    DoubleFinalize,
    #[error("Failed to fetch identifier data for struct {0}.")]
    InnerBuiltinPtrsIdentifierMissing(String),
    #[error("The `members` field is None in identifier data for struct {0}.")]
    MissingMembers(String),
    #[error("SyscallTrace should be finalized before accessing resources.")]
    ResourceAccessBeforeFinalize,
    #[error("{0}")]
    UnknownBuiltin(String),
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

#[allow(dead_code)]
pub struct ResourceCounter {
    n_steps: usize,
    range_check_ptr: Relocatable,
    builtin_ptrs_dict: HashMap<BuiltinName, Relocatable>,
}

impl ResourceCounter {
    #[allow(dead_code)]
    pub(crate) fn new(
        n_steps: usize,
        range_check_ptr: Relocatable,
        ids_data: &HashMap<String, HintReference>,
        vm: &VirtualMachine,
        ap_tracking: &ApTracking,
        os_program: &Program,
    ) -> OsLoggerResult<Self> {
        Ok(Self {
            n_steps,
            range_check_ptr,
            builtin_ptrs_dict: Self::build_builtin_ptrs_dict(
                ids_data,
                vm,
                ap_tracking,
                os_program,
            )?,
        })
    }

    fn build_builtin_ptrs_dict(
        ids_data: &HashMap<String, HintReference>,
        vm: &VirtualMachine,
        ap_tracking: &ApTracking,
        os_program: &Program,
    ) -> OsLoggerResult<HashMap<BuiltinName, Relocatable>> {
        let mut builtin_ptrs_dict: HashMap<BuiltinName, Relocatable> = HashMap::new();

        // The `BuiltinPointers` struct has two fields: selectable and non-selectable builtins.
        Self::insert_builtins(
            "selectable",
            CairoStruct::SelectableBuiltins,
            &mut builtin_ptrs_dict,
            ids_data,
            vm,
            ap_tracking,
            os_program,
        )?;
        Self::insert_builtins(
            "non_selectable",
            CairoStruct::NonSelectableBuiltins,
            &mut builtin_ptrs_dict,
            ids_data,
            vm,
            ap_tracking,
            os_program,
        )?;

        Ok(builtin_ptrs_dict)
    }

    fn insert_builtins(
        inner_field_name: &str,
        inner_field_type: CairoStruct,
        builtin_ptrs_dict: &mut HashMap<BuiltinName, Relocatable>,
        ids_data: &HashMap<String, HintReference>,
        vm: &VirtualMachine,
        ap_tracking: &ApTracking,
        os_program: &Program,
    ) -> OsLoggerResult<()> {
        // We want all pointers except `segment_arena` and `sha256`.
        let excluded_builtins = ["segment_arena", "sha256"];
        let inner_struct_name: &str = inner_field_type.into();
        let inner_members = os_program
            .get_identifier(inner_struct_name)
            .ok_or(OsLoggerError::InnerBuiltinPtrsIdentifierMissing(inner_struct_name.into()))?
            .members
            .as_ref()
            .ok_or(OsLoggerError::MissingMembers(inner_struct_name.into()))?;

        for member_name in inner_members.keys() {
            if excluded_builtins.contains(member_name) {
                continue;
            }
            let member_ptr = get_address_of_nested_fields(
                ids_data,
                Ids::BuiltinPtrs,
                CairoStruct::BuiltinPointersPtr,
                vm,
                ap_tracking,
                &[inner_field_name, member_name.as_str()],
                os_program,
            )
            .map_err(OsLoggerError::BuiltinPtrs)?;
            builtin_ptrs_dict.insert(
                BuiltinName::from_str(member_name)
                    .ok_or_else(|| OsLoggerError::UnknownBuiltin(member_name.clone()))?,
                member_ptr,
            );
        }
        Ok(())
    }
}
