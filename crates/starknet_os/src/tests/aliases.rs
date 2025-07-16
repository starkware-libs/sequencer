use core::panic;
use std::collections::{BTreeSet, HashMap, HashSet};

use apollo_starknet_os_program::test_programs::ALIASES_TEST_BYTES;
use blockifier::state::stateful_compression::{ALIAS_COUNTER_STORAGE_KEY, INITIAL_AVAILABLE_ALIAS};
use blockifier::state::stateful_compression_test_utils::decompress;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::ALIAS_CONTRACT_ADDRESS;
use cairo_vm::hint_processor::builtin_hint_processor::dict_hint_utils::DICT_ACCESS_SIZE;
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_api::core::{ContractAddress, L2_ADDRESS_UPPER_BOUND};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::io::os_output::OsStateDiff;
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
    allocate_squashed_cairo_dict,
    flatten_cairo_dict,
    get_entrypoint_runner_config,
    parse_squashed_cairo_dict,
    test_cairo_function,
};

// TODO(Nimrod): Move this next to the stateful compression hints implementation.

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
        EndpointArg::Value(ValueArg::Single(Felt::ZERO.into())), // Aliases.len
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
    let [
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(n_aliases))),
        EndpointArg::Pointer(PointerArg::Array(aliases_storage_updates)),
        EndpointArg::Pointer(PointerArg::Array(alias_per_key)),
    ] = explicit_return_values.as_slice()
    else {
        panic!(
            "The return value doesn't match the given format.\n Got: {explicit_return_values:?}"
        );
    };
    let n_aliases = felt_to_usize(n_aliases).unwrap();
    assert_eq!(n_aliases, aliases_storage_updates.len() / DICT_ACCESS_SIZE);
    let aliases_storage_updates_as_felts: Vec<Felt> =
        aliases_storage_updates.iter().map(|f| f.get_int().unwrap()).collect();
    let actual_alias_storage = parse_squashed_cairo_dict(&aliases_storage_updates_as_felts);
    let alias_per_key: Vec<Felt> = alias_per_key.iter().map(|f| f.get_int().unwrap()).collect();
    (actual_alias_storage, alias_per_key)
}

#[rstest]
#[case::non_allocation_of_address_lt_16_from_empty_storage(
    HashMap::from([
        (
            15,
            HashMap::from([(5534, 1), (98435, 1), (99999, 1)])
        ),
        (
            16,
            HashMap::from([(11, 1), (127, 1), (128, 1), (129, 1), (225, 1), (7659, 1)])
        ),
        (
            7659,
            HashMap::from([(12, 0), (200, 1), (300, 1), (1111, 1)])
        ),
        (
            99999,
            HashMap::from([(225, 1)])
        )
    ]),
    HashMap::new(),
    HashMap::new(),
    HashMap::new(),
    HashMap::from([(0, 136), (128, 128), (129, 129), (200, 132), (225, 130), (300, 133), (1111, 134), (7659, 131), (99999, 135)]),
    97,
)]
#[case::non_allocation_of_address_lt_16_from_non_empty_storage(
    HashMap::from([
        (
            9,
            HashMap::from([(5534, 1), (98435, 1), (99999, 1)])
        ),
        (
            44,
            HashMap::from([(11, 1), (129, 1), (225, 1), (400, 1), (7659, 1)])
        ),
        (
            400,
            HashMap::from([(225, 1), (400, 1), (700, 1), (701, 1), (1111, 1)])
        ),
    ]),
    HashMap::new(),
    HashMap::new(),
    HashMap::from([(0, 135), (129, 128), (225, 129), (7659, 130), (200, 131), (300, 132), (1111, 133), (99999, 134)]),
    HashMap::from([(0, 138), (129, 128), (225, 129), (400, 135), (700, 136), (701, 137), (1111, 133), (7659, 130)]),
    76,
)]
#[case::non_allocation_with_only_trivial_updates(
    HashMap::from([
        (
            11,
            HashMap::from([(5534, 1), (98435, 1), (99999, 1)])
        ),
        (
            44,
            HashMap::from([(11, 0), (129, 1), (225, 0), (400, 1), (7659, 1)])
        ),
        (
            400,
            HashMap::from([(225, 0), (406, 0), (700, 1), (701, 1), (1111, 1)])
        ),
        (
            598,
            HashMap::from([(2255, 0), (7008, 0)]) // Trivial update.
        )
    ]),
    HashMap::new(),
    HashMap::new(),
    HashMap::new(),
    HashMap::from([(0, 134), (129, 128), (400, 129), (700, 131), (701, 132), (1111, 133), (7659, 130)]),
    73,
)]
#[case::allocation_with_only_nonce_change(
    HashMap::new(),
    HashMap::from([
        (13, 1),
        (58, 1),
        (11111, DEFAULT_CLASS_HASH), // Gets a new nonce.
        (222222, 1),
        (3333333, 1),
        (3333336, DEFAULT_CLASS_HASH), // Nothing changed.
    ]),
    HashMap::from([(11111, 1)]),
    HashMap::new(),
    HashMap::from([(0, 131), (11111, 128), (222222, 129), (3333333, 130)]),
    49,
)]
#[case::non_allocation_with_trivial_class_hash_update(
    HashMap::new(),
    HashMap::from([(24, 1), (5000, 1), (6666, 1), (9999, 1), (11111, DEFAULT_CLASS_HASH),
    ]),
    HashMap::new(),
    HashMap::from([(0, 133), (5000, 128), (11111, 129), (222222, 130), (3333333, 131), (87777, 132)]),
    HashMap::from([(0, 135), (5000, 128), (6666, 133), (9999, 134)]),
    40,
)]
#[case::allocation_with_partially_trivial_updates(
    HashMap::from([
        (
            1,
            HashMap::from([(777, 1), (8888, 1), (9999, 1)]) // No aliases.
        ),
        (
            100,
            HashMap::from([(200, 1), (777, 1), (888, 0)]) // Aliases for non-trivial diffs.
        ),
        (
            600,
            HashMap::from([(2000, 1), (3000, 1)])
        ),
        (
            800,
            HashMap::from([(700, 1), (701, 1)])
        ),
        (
            3000,
            HashMap::from([(600, 1), (2000, 1)])
        ),
        (
            10000,
            HashMap::from([(34567, 0), (435, 0)])  // No aliases (all diffs trivial).
        )
    ]),
    HashMap::from([(200, 1), (500, 1), (700, 1), (800, DEFAULT_CLASS_HASH)]),
    HashMap::from([(700, 1), (10000, 0)]),
    HashMap::new(),
    HashMap::from([(0, 137), (200, 128), (500, 130), (600, 133), (700, 134), (701, 135), (777, 129), (800, 136), (2000, 131), (3000, 132)]),
    118
)]
fn test_allocate_addresses_for_state_diff_and_replace(
    #[case] storage_updates: HashMap<u128, HashMap<u128, u128>>,
    #[case] address_to_class_hash: HashMap<u128, u128>,
    #[case] address_to_nonce: HashMap<u128, u128>,
    #[case] initial_alias_storage: HashMap<u128, u128>,
    #[case] expected_alias_storage: HashMap<u128, u128>,
    #[case] contract_state_diff_len: usize,
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
            allocate_squashed_cairo_dict(&HashMap::new(), &inner_updates, &mut cairo_runner.vm);
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
    let storage_view = initial_alias_storage
        .iter()
        .map(|(key, value)| ((*ALIAS_CONTRACT_ADDRESS, (*key).into()), (*value).into()))
        .collect();
    let state_reader = DictStateReader { storage_view, ..Default::default() };
    let expected_aliases_storage_flat_length = expected_alias_storage.len() * DICT_ACCESS_SIZE;
    let expected_explicit_return_values = vec![
        EndpointArg::Pointer(PointerArg::Array(vec![
            MaybeRelocatable::Int(Felt::ZERO);
            expected_aliases_storage_flat_length
        ])),
        EndpointArg::Pointer(PointerArg::Array(vec![
            MaybeRelocatable::Int(Felt::ZERO);
            contract_state_diff_len
        ])),
        EndpointArg::Pointer(PointerArg::Array(vec![
            MaybeRelocatable::Int(Felt::ZERO);
            contract_state_diff_len
        ])),
    ];

    // Run the entrypoint with validations on the explicit & implicit args.
    let skip_parameter_validations = false;
    let (_, explicit_return_values) = run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        Some(state_reader),
        &mut cairo_runner,
        &program,
        &runner_config,
        &expected_explicit_return_values,
        skip_parameter_validations,
    )
    .unwrap();

    let [
        EndpointArg::Pointer(PointerArg::Array(aliases_storage_updates)),
        EndpointArg::Pointer(PointerArg::Array(contract_state_diff)),
        EndpointArg::Pointer(PointerArg::Array(contract_state_diff_with_aliases)),
    ] = explicit_return_values.as_slice()
    else {
        panic!(
            "The return value doesn't match the given format.\n Got: {explicit_return_values:?}"
        );
    };

    // Compare the aliases storage updates.
    let aliases_storage_updates_as_felts: Vec<Felt> =
        aliases_storage_updates.iter().map(|f| f.get_int().unwrap()).collect();
    let actual_alias_storage = parse_squashed_cairo_dict(&aliases_storage_updates_as_felts);
    let expected_alias_storage_felts: HashMap<Felt, Felt> = expected_alias_storage
        .iter()
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect();
    assert_eq!(actual_alias_storage, expected_alias_storage_felts);

    // Parse the OS output.
    let contract_state_diff_as_felts: Vec<Felt> = contract_state_diff
        .iter()
        .map(|f| f.get_int().unwrap())
        .chain([Felt::ZERO]) // Number of declared classes, zero in this case.
        .collect();
    let contract_state_diff_with_aliases_as_felts: Vec<Felt> = contract_state_diff_with_aliases
        .iter()
        .map(|f| f.get_int().unwrap())
        .chain([Felt::ZERO]) // Number of declared classes, zero in this case.
        .collect();
    let full_output = true;
    let os_state_diff =
        OsStateDiff::from_iter(&mut contract_state_diff_as_felts.into_iter(), full_output).unwrap();
    let os_state_diff_with_aliases = OsStateDiff::from_iter(
        &mut contract_state_diff_with_aliases_as_felts.into_iter(),
        full_output,
    )
    .unwrap();

    // Sanity check - make sure the alias allocation is not trivial.
    assert!(os_state_diff != os_state_diff_with_aliases);
    let state_maps_with_aliases = os_state_diff_with_aliases.as_state_maps();

    // A new state reader is created because the previous one was moved into the hint processor.
    let storage_view: HashMap<(ContractAddress, StorageKey), Felt> = initial_alias_storage
        .into_iter()
        .chain(expected_alias_storage.into_iter())
        .map(|(key, value)| ((*ALIAS_CONTRACT_ADDRESS, key.into()), value.into()))
        .collect();
    let alias_keys = storage_view.keys().map(|(_, key)| *key).collect();
    let state = DictStateReader { storage_view, ..Default::default() };
    let decompressed_state_maps =
        decompress(&state_maps_with_aliases, &state, *ALIAS_CONTRACT_ADDRESS, alias_keys);

    assert_eq!(decompressed_state_maps, os_state_diff.as_state_maps());
}
