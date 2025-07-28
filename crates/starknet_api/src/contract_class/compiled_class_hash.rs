use cairo_lang_starknet_classes::casm_contract_class::CasmContractEntryPoint;
use itertools::Itertools;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::StarkHash;

/// A trait for types that can be hashed as an entry point.
pub trait EntryPointHashable {
    fn get_selector(&self) -> Felt;
    fn get_offset(&self) -> Felt;
    fn get_builtins(&self) -> Vec<Felt>;
}

#[allow(dead_code)]
fn entry_point_hash<H: StarkHash, EH: EntryPointHashable>(entry_points: &[EH]) -> Felt {
    let mut entry_point_hash_elements = vec![];
    for entry_point in entry_points {
        entry_point_hash_elements.push(entry_point.get_selector());
        entry_point_hash_elements.push(entry_point.get_offset());
        entry_point_hash_elements.push(H::hash_array(&entry_point.get_builtins()));
    }
    H::hash_array(&entry_point_hash_elements)
}

impl EntryPointHashable for CasmContractEntryPoint {
    fn get_selector(&self) -> Felt {
        Felt::from(self.selector.clone())
    }
    fn get_offset(&self) -> Felt {
        Felt::from(self.offset)
    }
    fn get_builtins(&self) -> Vec<Felt> {
        self.builtins
            .iter()
            .map(|builtin| Felt::from_bytes_be_slice(builtin.as_bytes()))
            .collect_vec()
    }
}
