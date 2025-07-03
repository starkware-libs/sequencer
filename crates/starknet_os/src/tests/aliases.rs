use std::collections::{HashMap, HashSet};

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
    run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};
use crate::test_utils::utils::{
    get_entrypoint_runner_config,
    parse_squashed_cairo_dict,
    test_cairo_function,
};

// TODO(Nimrod): Move this next to the stateful compression hints implementation.
// TODO(Amos): This test is incomplete. Add the rest of the test cases and remove this todo.

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
    let (_, explicit_return_values, _) = run_cairo_0_entry_point(
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
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(n_aliases))),
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
