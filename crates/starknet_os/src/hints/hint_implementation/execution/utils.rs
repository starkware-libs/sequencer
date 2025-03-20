use std::collections::HashMap;

use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Resource,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::vars::CairoStruct;
use crate::vm_utils::{insert_values_to_fields, IdentifierGetter, LoadCairoObject};

impl<IG: IdentifierGetter> LoadCairoObject<IG> for ValidResourceBounds {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        match self {
            ValidResourceBounds::L1Gas(l1_gas_bounds) => {
                l1_gas_bounds.load_into(vm, identifier_getter, address, constants)
            }
            ValidResourceBounds::AllResources(all_resource_bounds) => {
                all_resource_bounds.load_into(vm, identifier_getter, address, constants)
            }
        }
    }
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for ResourceBounds {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        _constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        insert_values_to_fields(
            address,
            CairoStruct::ResourceBounds,
            vm,
            &get_resource_bounds_list(Resource::L1Gas, self),
            identifier_getter,
        )
    }
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for AllResourceBounds {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        _constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        let mut resource_bounds_list: Vec<(String, MaybeRelocatable)> = vec![];
        for resource in [Resource::L1Gas, Resource::L2Gas, Resource::L1DataGas] {
            resource_bounds_list
                .extend(get_resource_bounds_list(resource, &self.get_bound(resource)));
        }
        insert_values_to_fields(
            address,
            CairoStruct::ResourceBounds,
            vm,
            &resource_bounds_list,
            identifier_getter,
        )
    }
}

fn get_resource_bounds_list(
    resource: Resource,
    resource_bounds: &ResourceBounds,
) -> Vec<(String, MaybeRelocatable)> {
    vec![
        (
            "resource_name".to_string(),
            Felt::from_hex(resource.to_hex())
                .expect("resource as hex expected to be converted into felt")
                .into(),
        ),
        ("max_amount".to_string(), Felt::from(resource_bounds.max_amount).into()),
        ("max_price_per_unit".to_string(), Felt::from(resource_bounds.max_price_per_unit).into()),
    ]
}
