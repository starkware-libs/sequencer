use std::any::Any;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::vec::IntoIter;

use cairo_vm::hint_processor::builtin_hint_processor::dict_hint_utils::DICT_ACCESS_SIZE;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use ethnum::U256;
use num_bigint::{BigInt, Sign};
use rand::rngs::StdRng;
use rand::SeedableRng;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::kzg::utils::BASE;
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    Cairo0EntryPointRunnerResult,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
};

#[allow(clippy::too_many_arguments)]
pub fn run_cairo_function_and_check_result(
    runner_config: &EntryPointRunnerConfig,
    program_bytes: &[u8],
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_explicit_retdata: &[EndpointArg],
    expected_implicit_retdata: &[EndpointArg],
    hint_locals: HashMap<String, Box<dyn Any>>,
) -> Cairo0EntryPointRunnerResult<()> {
    let state_reader = None;
    let (actual_implicit_retdata, actual_explicit_retdata, _) =
        initialize_and_run_cairo_0_entry_point(
            runner_config,
            program_bytes,
            function_name,
            explicit_args,
            implicit_args,
            expected_explicit_retdata,
            hint_locals,
            state_reader,
        )?;
    assert_eq!(expected_explicit_retdata, &actual_explicit_retdata);
    assert_eq!(expected_implicit_retdata, &actual_implicit_retdata);
    Ok(())
}

/// Creates a squashed dict from previous and new values, and stores it in a new memory segment.
pub fn allocate_squashed_cairo_dict(
    prev_values: &HashMap<Felt, MaybeRelocatable>,
    new_values: &HashMap<Felt, MaybeRelocatable>,
    vm: &mut VirtualMachine,
) -> (Relocatable, Relocatable) {
    let squashed_dict = flatten_cairo_dict(prev_values, new_values);
    let dict_segment_start = vm.add_memory_segment();
    let dict_segment_end = vm.load_data(dict_segment_start, &squashed_dict).unwrap();
    (dict_segment_start, dict_segment_end)
}

pub fn flatten_cairo_dict(
    prev_values: &HashMap<Felt, MaybeRelocatable>,
    new_values: &HashMap<Felt, MaybeRelocatable>,
) -> Vec<MaybeRelocatable> {
    let mut squashed_dict = vec![];
    let mut sorted_new_values: Vec<_> = new_values.iter().collect();
    sorted_new_values.sort_by_key(|(key, _)| *key);

    for (key, value) in sorted_new_values {
        let prev_value = prev_values.get(key).unwrap_or(&MaybeRelocatable::Int(Felt::ZERO));
        squashed_dict.push((*key).into());
        squashed_dict.push(prev_value.clone());
        squashed_dict.push(value.clone());
    }
    squashed_dict
}

pub fn parse_squashed_cairo_dict(squashed_dict: &[Felt]) -> HashMap<Felt, Felt> {
    assert!(squashed_dict.len() % DICT_ACCESS_SIZE == 0, "Invalid squashed dict length");
    let key_offset = 0;
    let new_val_offset = 2;
    squashed_dict
        .chunks(DICT_ACCESS_SIZE)
        .map(|chunk| (chunk[key_offset], chunk[new_val_offset]))
        .collect()
}

// 2**251 + 17 * 2**192 + 1
pub static DEFAULT_PRIME: LazyLock<BigInt> = LazyLock::new(|| {
    BigInt::from_bytes_be(
        Sign::Plus,
        &(U256::from(2_u32).pow(251) + 17 * U256::from(2_u32).pow(192) + 1).to_be_bytes(),
    )
});

#[allow(clippy::too_many_arguments, dead_code)]
pub(crate) fn test_cairo_function(
    runner_config: &EntryPointRunnerConfig,
    program_bytes: &[u8],
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_explicit_retdata: &[EndpointArg],
    expected_implicit_retdata: &[EndpointArg],
    hint_locals: HashMap<String, Box<dyn Any>>,
) {
    run_cairo_function_and_check_result(
        runner_config,
        program_bytes,
        function_name,
        explicit_args,
        implicit_args,
        expected_explicit_retdata,
        expected_implicit_retdata,
        hint_locals,
    )
    .unwrap();
}

#[allow(dead_code)]
pub(crate) fn seeded_random_prng() -> StdRng {
    StdRng::seed_from_u64(42)
}

/// Returns the lift of the given field element, val, as a `BigInt` in the range
/// (-prime/2, prime/2).
// TODO(Amos): Use cairo VM version if it is made public:
// https://github.com/lambdaclass/cairo-vm/blob/052e7cef977b336305c869fccbf24e1794b116ff/vm/src/hint_processor/builtin_hint_processor/kzg_da/mod.rs#L90
fn as_int(val: &Felt, prime: &BigInt) -> BigInt {
    let val = val.to_bigint();
    if val < (prime / BigInt::from(2)) {
        return val.clone();
    }
    val - prime
}

/// Takes a BigInt3 struct represented by the limbs (d0, d1, d2) of
/// and reconstructs the corresponding integer (see split_bigint3()).
/// Note that the limbs do not have to be in the range [0, BASE).
/// Prime is used to handle negative values of the limbs.
// TODO(Amos): Use cairo VM version if it is made public:
// https://github.com/lambdaclass/cairo-vm/blob/052e7cef977b336305c869fccbf24e1794b116ff/vm/src/hint_processor/builtin_hint_processor/kzg_da/mod.rs#L99
pub fn pack_bigint3(limbs: &[Felt]) -> BigInt {
    assert!(limbs.len() == 3, "Expected 3 limbs, got {}", limbs.len());
    limbs.iter().enumerate().fold(BigInt::ZERO, |acc, (i, &limb)| {
        acc + as_int(&limb, &DEFAULT_PRIME) * BASE.pow(i.try_into().unwrap())
    })
}

pub(crate) fn get_entrypoint_runner_config() -> EntryPointRunnerConfig {
    EntryPointRunnerConfig {
        layout: LayoutName::small,
        add_main_prefix_to_entrypoint: false,
        ..Default::default()
    }
}

pub(crate) type DictEntry = [Felt; DICT_ACCESS_SIZE];

#[derive(Debug, PartialEq, Eq)]
/// Represents a contract's state diff outputted by the OS in `full_output` mode.
pub(crate) struct FullOutputContractChanges {
    pub(crate) contract_address: Felt,
    pub(crate) prev_nonce: Felt,
    pub(crate) new_nonce: Felt,
    pub(crate) prev_class_hash: Felt,
    pub(crate) new_class_hash: Felt,
    pub(crate) storage_changes: Vec<DictEntry>,
}

impl FullOutputContractChanges {
    pub(crate) fn new_from_iter(iter: &mut IntoIter<Felt>) -> Self {
        let contract_address = iter.next().unwrap();
        let prev_nonce = iter.next().unwrap();
        let new_nonce = iter.next().unwrap();
        let prev_class_hash = iter.next().unwrap();
        let new_class_hash = iter.next().unwrap();
        let n_storage_changes = usize::try_from(iter.next().unwrap()).unwrap();
        let storage_changes = parse_dict_entries(n_storage_changes, iter);
        FullOutputContractChanges {
            contract_address,
            prev_nonce,
            new_nonce,
            prev_class_hash,
            new_class_hash,
            storage_changes,
        }
    }
}

pub(crate) fn parse_dict_entries(
    n_entries: usize,
    entries_iter: &mut IntoIter<Felt>,
) -> Vec<DictEntry> {
    (0..n_entries)
        .map(|_| {
            let key = entries_iter.next().unwrap();
            let prev_value = entries_iter.next().unwrap();
            let value = entries_iter.next().unwrap();
            [key, prev_value, value]
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FullOutputOsStateDiff {
    pub(crate) contracts: Vec<FullOutputContractChanges>,
    pub(crate) classes: Vec<DictEntry>,
}

impl FullOutputOsStateDiff {
    pub(crate) fn new_from_iter(os_output_iter: &mut IntoIter<Felt>) -> Self {
        let n_contracts = usize::try_from(os_output_iter.next().unwrap()).unwrap();
        let contracts = (0..n_contracts)
            .map(|_| FullOutputContractChanges::new_from_iter(os_output_iter))
            .collect();
        let n_classes = usize::try_from(os_output_iter.next().unwrap()).unwrap();
        let classes = parse_dict_entries(n_classes, os_output_iter);
        Self { contracts, classes }
    }
}
