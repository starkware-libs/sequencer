use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use cairo_vm::hint_processor::builtin_hint_processor::dict_manager::DictManager;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    get_relocatable_from_var_name,
    insert_value_from_var_name,
};
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;
use crate::hints::vars::{Const, Ids, Scope};

/// Hint enum maps between a (python) hint string in the cairo OS program under cairo-lang to a
/// matching enum variant defined in the crate.
pub trait HintEnum {
    fn from_str(hint_str: &str) -> Result<Self, OsHintError>
    where
        Self: Sized;

    fn to_str(&self) -> &'static str;
}

pub struct HintContext<'a> {
    pub vm: &'a mut VirtualMachine,
    pub exec_scopes: &'a mut ExecutionScopes,
    pub ids_data: &'a HashMap<String, HintReference>,
    pub ap_tracking: &'a ApTracking,
    pub constants: &'a HashMap<String, Felt>,
}

impl HintContext<'_> {
    pub fn insert_value<T: Into<MaybeRelocatable>>(
        &mut self,
        var_id: Ids,
        value: T,
    ) -> Result<(), HintError> {
        insert_value_from_var_name(var_id.into(), value, self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn get_integer(&self, var_id: Ids) -> Result<Felt, HintError> {
        get_integer_from_var_name(var_id.into(), self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn get_ptr(&self, var_id: Ids) -> Result<Relocatable, HintError> {
        get_ptr_from_var_name(var_id.into(), self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn get_relocatable(&self, var_id: Ids) -> Result<Relocatable, HintError> {
        get_relocatable_from_var_name(var_id.into(), self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn fetch_as<T: TryFrom<Felt>>(&self, var_id: Ids) -> Result<T, OsHintError>
    where
        <T as TryFrom<Felt>>::Error: std::fmt::Debug,
    {
        let felt = self.get_integer(var_id)?;
        T::try_from(felt).map_err(|error| OsHintError::IdsConversion {
            variant: var_id,
            felt,
            ty: std::any::type_name::<T>().into(),
            reason: format!("{error:?}"),
        })
    }

    // TODO(Yoni): consider removing the fetch functions from Const.
    pub fn fetch_const(&self, constant: Const) -> Result<&Felt, HintError> {
        constant.fetch(self.constants)
    }

    pub fn fetch_const_as<T: TryFrom<Felt>>(&self, constant: Const) -> Result<T, OsHintError>
    where
        <T as TryFrom<Felt>>::Error: std::fmt::Debug,
    {
        constant.fetch_as(self.constants)
    }

    // Scope helper methods.

    /// Gets a value from the execution scopes.
    pub(crate) fn get_from_scope<T: Clone + 'static>(&self, scope: Scope) -> Result<T, HintError> {
        self.exec_scopes.get(scope.into())
    }

    /// Inserts a value into the execution scopes.
    pub(crate) fn insert_into_scope<T: Clone + Send + Sync + 'static>(
        &mut self,
        scope: Scope,
        value: T,
    ) {
        self.exec_scopes.insert_value(scope.into(), value)
    }

    /// Gets the dict manager from the execution scopes.
    pub(crate) fn get_dict_manager(&self) -> Result<Rc<RefCell<DictManager>>, HintError> {
        self.exec_scopes.get_dict_manager()
    }
}
