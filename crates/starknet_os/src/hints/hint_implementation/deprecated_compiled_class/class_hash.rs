use std::borrow::Cow;
use std::collections::HashMap;
use std::marker::PhantomData;

use starknet_api::contract_class::EntryPointType;
use starknet_api::deprecated_contract_class::{ContractClass, EntryPointV0};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Pedersen, StarkHash};

use crate::hints::class_hash::hinted_class_hash::{
    compute_cairo_hinted_class_hash,
    CairoContractDefinition,
    HintedClassHashError,
};
use crate::hints::hint_implementation::deprecated_compiled_class::utils::DEPRECATED_COMPILED_CLASS_VERSION;

/// Mimics the behavior of the cairo hash state.
struct HashState<T: StarkHash> {
    pub(crate) current_hash: Felt,
    n_words: usize,
    _marker: PhantomData<T>,
}

impl<T: StarkHash> HashState<T> {
    fn new() -> Self {
        Self { current_hash: Felt::ZERO, n_words: 0, _marker: PhantomData }
    }

    fn update_single(&mut self, item: &Felt) {
        self.current_hash = T::hash(&self.current_hash, item);
        self.n_words += 1;
    }

    fn update_with_hashchain(&mut self, data: &[Felt]) {
        let data_hash = data.iter().fold(Felt::ZERO, |acc, x| T::hash(&acc, x));
        self.update_single(&T::hash(&data_hash, &data.len().into()));
    }

    /// Consume self, to prevent further updates.
    fn finalize(self) -> Felt {
        T::hash(&self.current_hash, &Felt::from(self.n_words))
    }
}

struct FlatEntryPointFelts {
    external: Vec<Felt>,
    l1_handler: Vec<Felt>,
    constructor: Vec<Felt>,
}

fn get_flat_entry_point_felts(
    entry_points_by_type: &HashMap<EntryPointType, Vec<EntryPointV0>>,
) -> FlatEntryPointFelts {
    fn flatten_entry_points(
        entry_points: &HashMap<EntryPointType, Vec<EntryPointV0>>,
        ty: EntryPointType,
    ) -> Vec<Felt> {
        entry_points
            .get(&ty)
            .unwrap_or(&vec![])
            .iter()
            .flat_map(|ep| [ep.selector.0, Felt::from(ep.offset.0)])
            .collect()
    }
    FlatEntryPointFelts {
        external: flatten_entry_points(entry_points_by_type, EntryPointType::External),
        l1_handler: flatten_entry_points(entry_points_by_type, EntryPointType::L1Handler),
        constructor: flatten_entry_points(entry_points_by_type, EntryPointType::Constructor),
    }
}

fn ascii_strs_as_felts<'a>(strs: &Vec<Cow<'a, str>>) -> Vec<Felt> {
    strs.iter().map(|s| Felt::from_bytes_be_slice(s.as_bytes())).collect()
}

fn hex_strs_as_felts<'a>(strs: &Vec<Cow<'a, str>>) -> Vec<Felt> {
    strs.iter().map(|s| Felt::from_hex_unchecked(s)).collect()
}

pub fn compute_deprecated_class_hash(
    contract_class: &ContractClass,
) -> Result<Felt, HintedClassHashError> {
    let hinted_class_hash = compute_cairo_hinted_class_hash(contract_class)?;
    let contract_definition_vec = serde_json::to_vec(contract_class)?;
    let contract_definition: CairoContractDefinition<'_> =
        serde_json::from_slice(&contract_definition_vec)?;

    let FlatEntryPointFelts { external, l1_handler, constructor } =
        get_flat_entry_point_felts(&contract_definition.entry_points_by_type);
    let builtins = ascii_strs_as_felts(&contract_definition.program.builtins);
    let bytecode = hex_strs_as_felts(&contract_definition.program.data);

    let mut hash_state = HashState::<Pedersen>::new();
    hash_state.update_single(&DEPRECATED_COMPILED_CLASS_VERSION);
    hash_state.update_with_hashchain(&external);
    hash_state.update_with_hashchain(&l1_handler);
    hash_state.update_with_hashchain(&constructor);
    hash_state.update_with_hashchain(&builtins);
    hash_state.update_single(&hinted_class_hash);
    hash_state.update_with_hashchain(&bytecode);
    Ok(hash_state.finalize())
}
