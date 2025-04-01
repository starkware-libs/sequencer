use std::collections::HashMap;

use cairo_vm::types::builtin_name::BuiltinName;
use starknet_os::test_utils::cairo_runner::{EndpointArg, ImplicitArg, PointerArg};
use starknet_os::test_utils::utils::create_squashed_cairo_dict;
use starknet_types_core::felt::Felt;

use crate::os_cli::tests::types::OsPythonTestResult;
use crate::os_cli::tests::utils::test_cairo_function;
use crate::{felt_to_felt_hashmap, felt_to_value_hashmap, felt_tuple, hashmap};

const DEFAULT_CLASS_HASH: i32 = 7777;

struct AllocateAliasesAndReplaceParams {
    storage_updates: HashMap<Felt, HashMap<Felt, Felt>>,
    address_to_class_hash: HashMap<Felt, Felt>,
    address_to_nonce: HashMap<Felt, Felt>,
    initial_alias_storage: HashMap<Felt, Felt>,
    expected_alias_storage: Vec<(Felt, Felt)>,
}

// TODO(Amos): This test is incomplete. Add the rest of the test cases and remove this todo.
pub(crate) fn aliases_test(input: &str) -> OsPythonTestResult {
    test_constants(input)?;
    test_allocate_aliases_and_replace(input)?;
    Ok("".to_string())
}

fn test_constants(input: &str) -> OsPythonTestResult {
    let max_non_compressed_contract_address = 15;
    let alias_counter_storage_key = 0;
    let initial_available_alias = 128;
    let alias_contract_address = 2;
    test_cairo_function(
        input,
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

fn test_allocate_aliases_and_replace(input: &str) -> OsPythonTestResult {
    for parameters in get_allocate_aliases_and_replace_params() {
        let AllocateAliasesAndReplaceParams {
            storage_updates,
            address_to_class_hash,
            address_to_nonce,
            ..
        } = parameters;
        let mut modified_contracts: Vec<_> = storage_updates
            .keys()
            .chain(address_to_class_hash.keys())
            .chain(address_to_nonce.keys())
            .cloned()
            .collect();
        modified_contracts.sort();
        let mut prev_state_entries: HashMap<Felt, EndpointArg> = HashMap::new();
        let mut new_state_entries: HashMap<Felt, EndpointArg> = HashMap::new();

        for address in modified_contracts.iter() {
            let empty_hashmap = hashmap!();
            let inner_updates = storage_updates.get(address).unwrap_or(&empty_hashmap);
            let inner_updates: HashMap<Felt, EndpointArg> =
                inner_updates.iter().map(|(k, v)| (*k, (*v).into())).collect();
            let (new_class_hash, prev_class_hash) = (
                *address_to_class_hash.get(address).unwrap_or(&DEFAULT_CLASS_HASH.into()),
                Felt::from(DEFAULT_CLASS_HASH),
            );
            let (new_nonce, prev_nonce) =
                (*address_to_nonce.get(address).unwrap_or(&Felt::ZERO), Felt::ZERO);

            let old_storage_pointer =
                create_squashed_cairo_dict(&felt_to_felt_hashmap!(), &inner_updates);

            prev_state_entries.insert(
                *address,
                EndpointArg::Pointer(PointerArg::Composed(vec![
                    prev_class_hash.into(),
                    old_storage_pointer,
                    prev_nonce.into(),
                ])),
            );
            new_state_entries.insert(
                *address,
                EndpointArg::Pointer(PointerArg::Composed(vec![
                    new_class_hash.into(),
                    0.into(), // FIXME: Change test to put end address here.
                    new_nonce.into(),
                ])),
            );
        }

        let contract_state_changes =
            create_squashed_cairo_dict(&prev_state_entries, &new_state_entries);
        let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];
        let explicit_args = [Felt::from(modified_contracts.len()).into(), contract_state_changes];

        // FIXME: Use actual number of range check uses.
        let expected_implicit_retdata: [EndpointArg; 1] = [1.into()]; // Number of range check uses.
        // FIXME: Use actual expected return data.
        let expected_explicit_retdata: [EndpointArg; 0] = [];

        test_cairo_function(
            input,
            "allocate_aliases_and_replace",
            &explicit_args,
            &implicit_args,
            &expected_explicit_retdata,
            &expected_implicit_retdata,
            HashMap::new(), // FIXME: Add mock alias storage.
        )?;
    }
    // FIXME: Add rest of verifications.
    Ok("".to_string())
}

// FIXME: Add rest of test cases.
fn get_allocate_aliases_and_replace_params() -> Vec<AllocateAliasesAndReplaceParams> {
    let storage_updates = felt_to_value_hashmap! {
        1 => felt_to_felt_hashmap! {777 => 1, 8888 => 1, 9999 => 1},
        100 => felt_to_felt_hashmap! {200 => 1, 777 => 1, 888 => 0},
        600 => felt_to_felt_hashmap! {2000 => 1, 3000 => 1},
        800 => felt_to_felt_hashmap! {700 => 1, 701 => 1},
        3000 => felt_to_felt_hashmap! {600 => 1, 2000 => 1},
        10000=> felt_to_felt_hashmap! {34567 => 0, 435 => 0},
    };
    let address_to_class_hash =
        felt_to_felt_hashmap! {200 => 1, 500 => 1, 700 => 1, 800 => DEFAULT_CLASS_HASH};
    let address_to_nonce = felt_to_felt_hashmap! {700 => 1, 10000 => 0};
    let initial_alias_storage = felt_to_felt_hashmap! {};
    let expected_alias_storage = vec![
        felt_tuple!(0, 137),
        felt_tuple!(200, 128),
        felt_tuple!(500, 130),
        felt_tuple!(600, 133),
        felt_tuple!(700, 134),
        felt_tuple!(701, 135),
        felt_tuple!(777, 129),
        felt_tuple!(800, 136),
        felt_tuple!(2000, 131),
        felt_tuple!(3000, 132),
    ];
    vec![AllocateAliasesAndReplaceParams {
        storage_updates,
        address_to_class_hash,
        address_to_nonce,
        initial_alias_storage,
        expected_alias_storage,
    }]
}
