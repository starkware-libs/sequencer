use cairo_lang_starknet_classes::contract_class::ContractEntryPoint;
use num_traits::ToBytes;
use starknet_api::core::EntryPointSelector;
use starknet_types_core::felt::Felt;

pub fn contract_entrypoint_to_entrypoint_selector(
    entrypoint: &ContractEntryPoint,
) -> EntryPointSelector {
    let selector_felt = Felt::from_bytes_be_slice(&entrypoint.selector.to_be_bytes());
    EntryPointSelector(selector_felt)
}
