use std::collections::{BTreeSet, HashMap, HashSet};

use apollo_starknet_os_program::test_programs::ALIASES_TEST_BYTES;
use blockifier::state::stateful_compression::{ALIAS_COUNTER_STORAGE_KEY, INITIAL_AVAILABLE_ALIAS};
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::ALIAS_CONTRACT_ADDRESS;
use cairo_vm::hint_processor::builtin_hint_processor::dict_hint_utils::DICT_ACCESS_SIZE;
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_api::core::L2_ADDRESS_UPPER_BOUND;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};
use crate::test_utils::utils::{
    create_squashed_cairo_dict,
    flatten_cairo_dict,
    get_entrypoint_runner_config,
    parse_squashed_cairo_dict,
    test_cairo_function,
};

// TODO(Nimrod): Move this next to the stateful compression hints implementation.
// TODO(Amos): This test is incomplete. Add the rest of the test cases and remove this todo.

const DEFAULT_CLASS_HASH: u128 = 7777;

#[test]
fn test_constants() {
    let max_non_compressed_contract_address = 15;
    let alias_counter_storage_key = 0;
    let initial_available_alias = 128;
    let alias_contract_address = 2;
    test_cairo_function(
        &EntryPointRunnerConfig::default(),
        ALIASES_TEST_BYTES,
        "test_constants",
        &[
            max_non_compressed_contract_address.into(),
            alias_counter_storage_key.into(),
            initial_available_alias.into(),
            alias_contract_address.into(),
        ],
        &[],
        &[],
        &[],
        HashMap::new(),
    )
}

#[rstest]
#[case(
    Vec::new(),
    Vec::new(),
    HashMap::from([(0.into(), 128.into())])
)]
#[case(
    vec![Felt::from(&*L2_ADDRESS_UPPER_BOUND)],
    vec![128],
    HashMap::from([
        (0.into(), 129.into()),
        (Felt::from(&*L2_ADDRESS_UPPER_BOUND), 128.into())
    ])
)]
#[case(
    vec![2000.into(), 1999999999.into(), 3000.into(), 2000.into()],
    vec![128, 129, 130, 128],
    HashMap::from([
        (0.into(), 131.into()),
        (2000.into(), 128.into()),
        (3000.into(), 130.into()),
        (1999999999.into(), 129.into())
    ])
)]
#[case(
    Vec::from_iter((0..128).map(Felt::from)),
    (0..128).collect::<Vec<_>>(),
    HashMap::from_iter([(0.into(), 128.into())])
)]
#[case(
    Vec::from_iter((0..129).map(Felt::from)),
    (0..129).collect::<Vec<_>>(),
    HashMap::from_iter([
        (0.into(), 129.into()),
        (128.into(), 128.into())
    ])
)]
#[case(
    vec![
        13.into(),
        500.into(),
        11.into(),
        2000.into(),
        2001.into(),
        13.into(),
        501.into(),
        98.into(),
        222.into(),
        2000.into(),
        127.into(),
        128.into()
    ],
    vec![13, 128, 11, 129, 130, 13, 131, 98, 132, 129, 127, 133],
    HashMap::from([
        (0.into(), 134.into()),
        (128.into(), 133.into()),
        (222.into(), 132.into()),
        (500.into(), 128.into()),
        (501.into(), 131.into()),
        (2000.into(), 129.into()),
        (2001.into(), 130.into())
    ])
)]
#[case(
    (0..150_u8)
        .map(|i| Felt::from(128) + Felt::TWO.pow(i))
        .chain((0..150_u8).map(|i| Felt::from(128) + Felt::TWO.pow(i)))
        .collect::<Vec<_>>(),
    (0..150_u128)
        .map(|i| i + 128)
        .chain((0..150_u128).map(|i| i + 128))
        .collect::<Vec<_>>(),
    HashMap::from_iter(
        (0..150_u128)
            .map(|i| (Felt::from(128) + Felt::TWO.pow(i), Felt::from(i + 128)))
            .chain([(0.into(), (128 + 150).into())])
    )
)]
fn allocate_and_replace_keys_from_empty_storage(
    #[case] keys: Vec<Felt>,
    #[case] expected_alias_per_key: Vec<u128>,
    #[case] expected_alias_storage: HashMap<Felt, Felt>,
) {
    let expected_alias_per_key: Vec<_> =
        expected_alias_per_key.into_iter().map(Felt::from).collect();
    let (actual_alias_storage, actual_alias_per_key) =
        allocate_aliases_for_keys_and_replace(keys, HashMap::new());
    assert_eq!(actual_alias_storage, expected_alias_storage);
    assert_eq!(actual_alias_per_key, expected_alias_per_key);
}

#[rstest]
#[case(
    vec![],
    vec![],
    HashMap::from([(0.into(), 128.into())]),
    HashMap::from([(0, 128)])
)]
#[case(
    vec![2000.into()],
    vec![128],
    HashMap::from([
        (0.into(), 131.into()),
        (2000.into(), 128.into())
    ]),
    HashMap::from([
        (0, 131),
        (2000, 128),
        (1999999999, 129),
        (3000, 130)
    ])
)]
#[case(
    vec![2001.into()],
    vec![131],
    HashMap::from([
        (0.into(), 132.into()),
        (2001.into(), 131.into())
    ]),
    HashMap::from([
        (0, 131),
        (2000, 128),
        (1999999999, 129),
        (3000, 130)
    ])
)]
#[case(
    vec![2001.into(), 2000.into(), 2005.into()],
    vec![131, 128, 132],
    HashMap::from([
        (0.into(), 133.into()),
        (2000.into(), 128.into()),
        (2001.into(), 131.into()),
        (2005.into(), 132.into())
    ]),
    HashMap::from([
        (0, 131),
        (2000, 128),
        (1999999999, 129),
        (3000, 130)
    ])
)]
#[case(
    vec![
        13.into(),
        500.into(),
        11.into(),
        2000.into(),
        89999.into(),
        13.into(),
        501.into(),
        98.into(),
        222.into(),
        501.into()
    ],
    vec![13, 128, 11, 129, 131, 13, 132, 98, 133, 132],
    HashMap::from([
        (0.into(), 134.into()),
        (222.into(), 133.into()),
        (500.into(), 128.into()),
        (501.into(), 132.into()),
        (2000.into(), 129.into()),
        (89999.into(), 131.into())
    ]),
    HashMap::from([
        (0, 131),
        (500, 128),
        (2000, 129),
        (2001, 130)
    ])
)]
fn allocate_and_replace_keys_from_non_empty_storage(
    #[case] keys: Vec<Felt>,
    #[case] expected_alias_per_key: Vec<u128>,
    #[case] expected_alias_storage: HashMap<Felt, Felt>,
    #[case] initial_storage: HashMap<u128, u128>,
) {
    let initial_storage = initial_storage
        .into_iter()
        .map(|(key, value)| (StorageKey::from(key), Felt::from(value)))
        .collect::<HashMap<_, _>>();
    let expected_alias_per_key: Vec<_> =
        expected_alias_per_key.into_iter().map(Felt::from).collect();
    let (actual_alias_storage, actual_alias_per_key) =
        allocate_aliases_for_keys_and_replace(keys, initial_storage);

    assert_eq!(actual_alias_storage, expected_alias_storage);
    assert_eq!(actual_alias_per_key, expected_alias_per_key);
}

fn allocate_aliases_for_keys_and_replace(
    keys: Vec<Felt>,
    initial_storage: HashMap<StorageKey, Felt>,
) -> (HashMap<Felt, Felt>, Vec<Felt>) {
    let runner_config = get_entrypoint_runner_config();
    let entrypoint = "__main__.allocate_alias_for_keys_and_replace";
    let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];
    let unique_keys: HashSet<Felt> = HashSet::from_iter(
        keys.iter()
            .filter(|key| key >= &&INITIAL_AVAILABLE_ALIAS)
            .copied()
            .chain([*ALIAS_COUNTER_STORAGE_KEY.key()]),
    );
    let expected_explicit_return_values = vec![
        EndpointArg::Value(ValueArg::Single(Felt::ZERO)), // Aliases.len
        EndpointArg::Pointer(PointerArg::Array(vec![ // Aliases.ptr
            MaybeRelocatable::Int(Felt::ZERO);
            (unique_keys.len()) * DICT_ACCESS_SIZE
            ])),
        // Aliases per-key ptr.
        EndpointArg::Pointer(PointerArg::Array(vec![
            MaybeRelocatable::Int(Felt::ZERO);
            keys.len()
        ])),
    ];
    let n_keys_arg = EndpointArg::Value(ValueArg::Single(keys.len().into()));
    let keys_arg = EndpointArg::Pointer(PointerArg::Array(
        keys.iter().cloned().map(MaybeRelocatable::from).collect(),
    ));
    let explicit_args = vec![n_keys_arg, keys_arg];
    let storage_view = initial_storage
        .into_iter()
        .map(|(key, value)| ((*ALIAS_CONTRACT_ADDRESS, key), value))
        .collect();

    let state_reader = DictStateReader { storage_view, ..Default::default() };
    let (_, explicit_return_values, _) = initialize_and_run_cairo_0_entry_point(
        &runner_config,
        ALIASES_TEST_BYTES,
        entrypoint,
        &explicit_args,
        &implicit_args,
        &expected_explicit_return_values,
        HashMap::new(),
        Some(state_reader),
    )
    .unwrap();
    if let [
        EndpointArg::Value(ValueArg::Single(n_aliases)),
        EndpointArg::Pointer(PointerArg::Array(aliases_storage_updates)),
        EndpointArg::Pointer(PointerArg::Array(alias_per_key)),
    ] = explicit_return_values.as_slice()
    {
        let n_aliases = felt_to_usize(n_aliases).unwrap();
        assert_eq!(n_aliases, aliases_storage_updates.len() / DICT_ACCESS_SIZE);
        let aliases_storage_updates_as_felts: Vec<Felt> =
            aliases_storage_updates.iter().map(|f| f.get_int().unwrap()).collect();
        let actual_alias_storage = parse_squashed_cairo_dict(&aliases_storage_updates_as_felts);
        let alias_per_key: Vec<Felt> = alias_per_key.iter().map(|f| f.get_int().unwrap()).collect();
        (actual_alias_storage, alias_per_key)
    } else {
        panic!(
            "The return value doesn't match the given format.\n Got: {explicit_return_values:?}"
        );
    }
}

#[rstest]
#[case(HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new())]
fn test_allocate_addresses_for_state_diff_and_replace(
    #[case] storage_updates: HashMap<u128, HashMap<u128, u128>>,
    #[case] address_to_class_hash: HashMap<u128, u128>,
    #[case] address_to_nonce: HashMap<u128, u128>,
    #[case] initial_alias_storage: HashMap<u128, u128>,
    #[case] expected_alias_storage: HashMap<u128, u128>,
) {
    let runner_config = get_entrypoint_runner_config();
    let entrypoint = "__main__.allocate_aliases_and_replace";
    let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];
    let modified_contracts: BTreeSet<_> = storage_updates
        .keys()
        .chain(address_to_class_hash.keys().chain(address_to_nonce.keys()))
        .collect();

    // Initialize the runner to be able to allocate segments.
    let (mut cairo_runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        ALIASES_TEST_BYTES,
        entrypoint,
        &implicit_args,
        HashMap::new(),
    )
    .unwrap();

    // Construct the contract state changes.
    let mut prev_state_entries = HashMap::new();
    let mut new_state_entries = HashMap::new();
    let n_contracts = modified_contracts.len();
    for address in modified_contracts {
        let inner_updates = storage_updates
            .get(address)
            .unwrap_or(&HashMap::new())
            .iter()
            .map(|(k, v)| ((*k).into(), Felt::from(*v).into()))
            .collect();
        let (new_nonce, prev_nonce) = (address_to_nonce.get(address).copied().unwrap_or(0), 0);
        let (new_class_hash, prev_class_hash) = (
            address_to_class_hash.get(address).copied().unwrap_or(DEFAULT_CLASS_HASH),
            DEFAULT_CLASS_HASH,
        );
        let (prev_storage_ptr, new_storage_ptr) =
            create_squashed_cairo_dict(&HashMap::new(), &inner_updates, &mut cairo_runner.vm);
        let new_state_entry: Vec<MaybeRelocatable> = vec![
            Felt::from(new_class_hash).into(),
            new_storage_ptr.into(),
            Felt::from(new_nonce).into(),
        ];
        let prev_state_entry: Vec<MaybeRelocatable> = vec![
            Felt::from(prev_class_hash).into(),
            prev_storage_ptr.into(),
            Felt::from(prev_nonce).into(),
        ];
        new_state_entries
            .insert((*address).into(), cairo_runner.vm.gen_arg(&new_state_entry).unwrap());
        prev_state_entries
            .insert((*address).into(), cairo_runner.vm.gen_arg(&prev_state_entry).unwrap());
    }
    let flat_contract_state_changes = flatten_cairo_dict(&prev_state_entries, &new_state_entries);
    let explicit_args = vec![
        EndpointArg::Value(ValueArg::Single(n_contracts.into())),
        EndpointArg::Pointer(PointerArg::Array(flat_contract_state_changes)),
    ];
    let expected_explicit_return_values = vec![]; // COMPLETE!
    let storage_view = initial_alias_storage
        .into_iter()
        .map(|(key, value)| ((*ALIAS_CONTRACT_ADDRESS, key.into()), value.into()))
        .collect();
    let state_reader = DictStateReader { storage_view, ..Default::default() };
    run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        Some(state_reader),
        &mut cairo_runner,
        &program,
        &runner_config,
        &expected_explicit_return_values,
    );
}
