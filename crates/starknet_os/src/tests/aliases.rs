use std::collections::{HashMap, HashSet};

use apollo_starknet_os_program::test_programs::ALIASES_TEST_BYTES;
use blockifier::state::stateful_compression::{ALIAS_COUNTER_STORAGE_KEY, INITIAL_AVAILABLE_ALIAS};
use blockifier::test_utils::dict_state_reader::DictStateReader;
use cairo_vm::hint_processor::builtin_hint_processor::dict_hint_utils::DICT_ACCESS_SIZE;
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::builtin_name::BuiltinName;
use rstest::rstest;
use starknet_api::core::{ContractAddress, L2_ADDRESS_UPPER_BOUND};
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
use crate::test_utils::utils::{get_entrypoint_runner_config, test_cairo_function};

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
            Felt::ZERO;
            (unique_keys.len()) * DICT_ACCESS_SIZE
            ])),
        EndpointArg::Pointer(PointerArg::Array(vec![Felt::ZERO; keys.len()])),
    ];
    let n_keys_arg = EndpointArg::Value(ValueArg::Single(keys.len().into()));
    let keys_arg = EndpointArg::Pointer(PointerArg::Array(keys));
    let explicit_args = vec![n_keys_arg, keys_arg];
    let alias_contract_address: ContractAddress = Felt::TWO.try_into().unwrap();
    let storage_view = initial_storage
        .into_iter()
        .map(|(key, value)| ((alias_contract_address, key), value))
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
    let mut actual_alias_storage = HashMap::new();
    if let [
        EndpointArg::Value(ValueArg::Single(n_aliases)),
        EndpointArg::Pointer(PointerArg::Array(aliases_storage_updates)),
        EndpointArg::Pointer(PointerArg::Array(alias_per_key)),
    ] = explicit_return_values.as_slice()
    {
        let n_aliases = felt_to_usize(n_aliases).unwrap();
        assert!(aliases_storage_updates.len() % DICT_ACCESS_SIZE == 0);
        assert!(aliases_storage_updates.len() / DICT_ACCESS_SIZE == n_aliases);
        let key_offset = 0;
        let new_value_offset = 2;
        for i in 0..n_aliases {
            let key = aliases_storage_updates[i * DICT_ACCESS_SIZE + key_offset];
            let new_value = aliases_storage_updates[i * DICT_ACCESS_SIZE + new_value_offset];
            actual_alias_storage.insert(key, new_value);
        }
        (actual_alias_storage, alias_per_key.clone().to_vec())
    } else {
        panic!(
            "The return value doesn't match the given format.\n Got: {explicit_return_values:?}"
        );
    }
}
