use std::collections::HashMap;

use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::transaction::fields::{
    valid_resource_bounds_as_felts,
    ResourceAsFelts,
    ValidResourceBounds,
};
use starknet_types_core::felt::Felt;

use crate::hints::vars::CairoStruct;
use crate::vm_utils::{
    insert_values_to_fields,
    CairoSized,
    IdentifierGetter,
    LoadCairoObject,
    VmUtilsError,
    VmUtilsResult,
};

impl<IG: IdentifierGetter> LoadCairoObject<IG> for ResourceAsFelts {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        _constants: &HashMap<String, Felt>,
    ) -> VmUtilsResult<()> {
        let resource_bounds_list = vec![
            ("resource_name", self.resource_name.into()),
            ("max_amount", self.max_amount.into()),
            ("max_price_per_unit", self.max_price_per_unit.into()),
        ];
        insert_values_to_fields(
            address,
            CairoStruct::ResourceBounds,
            vm,
            &resource_bounds_list,
            identifier_getter,
        )
    }
}

impl<IG: IdentifierGetter> CairoSized<IG> for ResourceAsFelts {
    fn size(_identifier_getter: &IG) -> usize {
        3
    }
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for ValidResourceBounds {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> VmUtilsResult<()> {
        valid_resource_bounds_as_felts(self, false)
            .map_err(VmUtilsError::ResourceBoundsParsing)?
            .load_into(vm, identifier_getter, address, constants)
    }
}
