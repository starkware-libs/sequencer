use cairo_lang_starknet_classes::contract_class::ContractEntryPoint;
use starknet_api::core::EntryPointSelector;
use starknet_types_core::felt::Felt;

pub fn contract_entrypoint_to_entrypoint_selector(
    entrypoint: &ContractEntryPoint,
) -> EntryPointSelector {
    EntryPointSelector(Felt::from(&entrypoint.selector))
}
