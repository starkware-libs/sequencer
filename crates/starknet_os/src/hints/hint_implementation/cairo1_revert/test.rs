use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::LazyLock;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::hint_processor::builtin_hint_processor::dict_manager::DictManager;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use itertools::Itertools;
use rstest::rstest;
use starknet_api::core::CONTRACT_ADDRESS_DOMAIN_SIZE;
use starknet_types_core::felt::Felt;

use crate::test_utils::cairo_dict::parse_contract_changes;
use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    ValueArg,
};

const CHANGE_CONTRACT_ENTRY: Felt = CONTRACT_ADDRESS_DOMAIN_SIZE;
static CHANGE_CLASS_ENTRY: LazyLock<Felt> = LazyLock::new(|| CHANGE_CONTRACT_ENTRY + Felt::ONE);

enum Operation {
    ChangeClass { class_hash: Felt },
    ChangeContract { contract_address: Felt },
    StorageWrite { address: Felt, value: Felt },
}

impl Operation {
    fn encode(&self) -> [MaybeRelocatable; 2] {
        match self {
            Self::ChangeClass { class_hash } => [(*CHANGE_CLASS_ENTRY).into(), class_hash.into()],
            Self::ChangeContract { contract_address } => {
                [CHANGE_CONTRACT_ENTRY.into(), contract_address.into()]
            }
            Self::StorageWrite { address, value } => [address.into(), value.into()],
        }
    }
}

#[rstest]
#[case::noop(vec![])]
#[case::write(vec![Operation::StorageWrite { address: Felt::from(7u8), value: Felt::from(7u8) }])]
#[case::multiple(vec![
    Operation::ChangeContract { contract_address: Felt::from(6u8) },
    Operation::ChangeContract { contract_address: Felt::from(6u8) },
    Operation::ChangeClass { class_hash: Felt::from(4u8) },
    Operation::StorageWrite { address: Felt::from(7u8), value: Felt::from(7u8) },
    Operation::ChangeContract { contract_address: Felt::from(2u8) },
    Operation::StorageWrite { address: Felt::from(2u8), value: Felt::from(7u8) },
    Operation::StorageWrite { address: Felt::from(8u8), value: Felt::from(4u8) },
])]
fn test_revert(#[case] test_vector: Vec<Operation>) {
    let initial_contract_address = Felt::from(5u8);
    let initial_class_hash = Felt::ONE;
    let mut current_contract_address = initial_contract_address;
    let mut contract_addresses = HashSet::from([initial_contract_address]);
    let mut expected_storages: HashMap<Felt, HashMap<Felt, Felt>> = HashMap::new();
    let mut expected_class_hashes = HashMap::new();

    for operation in test_vector.iter().rev() {
        match operation {
            Operation::ChangeClass { class_hash } => {
                expected_class_hashes.insert(current_contract_address, class_hash);
            }
            Operation::ChangeContract { contract_address } => {
                current_contract_address = *contract_address;
                contract_addresses.insert(*contract_address);
            }
            Operation::StorageWrite { address, value } => {
                expected_storages
                    .entry(current_contract_address)
                    .or_default()
                    .insert(*address, *value);
            }
        }
    }

    // Initialize the runner.
    // Pass no implicits, as the runner initialization only requires the implicit builtins; the
    // implicit state_changes arg is added later.
    let runner_config = EntryPointRunnerConfig {
        trace_enabled: false,
        verify_secure: true,
        layout: LayoutName::starknet,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false,
    };
    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        OS_PROGRAM_BYTES,
        "starkware.starknet.core.os.execution.revert.handle_revert",
        &[],
        HashMap::new(),
    )
    .unwrap();

    // Create the implicit argument (contract state changes) for the runner.
    let state_changes: HashMap<MaybeRelocatable, MaybeRelocatable> = contract_addresses
        .iter()
        .sorted()
        .map(|address| {
            let state_entry: Vec<MaybeRelocatable> = vec![
                initial_class_hash.into(),
                runner.vm.add_memory_segment().into(), // storage_ptr
                Felt::ZERO.into(),                     // nonce
            ];
            (address.into(), runner.vm.gen_arg(&state_entry).unwrap())
        })
        .collect();

    // Add the state changes dict to the dict manager.
    let contract_state_changes = if let Ok(dict_manager) = runner.exec_scopes.get_dict_manager() {
        dict_manager.borrow_mut().new_dict(&mut runner.vm, state_changes).unwrap()
    } else {
        let mut dict_manager = DictManager::new();
        let base = dict_manager.new_dict(&mut runner.vm, state_changes).unwrap();
        runner.exec_scopes.insert_value("dict_manager", Rc::new(RefCell::new(dict_manager)));
        base
    };
    insert_value_into_ap(&mut runner.vm, contract_state_changes.clone()).unwrap();

    // Construct the revert log.
    let revert_log: Vec<MaybeRelocatable> =
        Operation::ChangeContract { contract_address: CONTRACT_ADDRESS_DOMAIN_SIZE }
            .encode()
            .into_iter()
            .chain(test_vector.iter().flat_map(|operation| operation.encode().into_iter()))
            .collect();
    let revert_log_end =
        runner.vm.gen_arg(&revert_log).unwrap().add_int(&revert_log.len().into()).unwrap();

    // Run the entrypoint.
    let explicit_args = vec![
        EndpointArg::Value(ValueArg::Single(initial_contract_address.into())),
        EndpointArg::Value(ValueArg::Single(revert_log_end)),
    ];
    let implicit_args = vec![ImplicitArg::NonBuiltin(EndpointArg::Value(ValueArg::Single(
        contract_state_changes.clone(),
    )))];
    let state_reader = None;
    let expected_explicit_return_values = vec![];
    let (implicit_return_values, _explicit_return_values) = run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        state_reader,
        &mut runner,
        &program,
        &runner_config,
        &expected_explicit_return_values,
    )
    .unwrap();

    // Run the entrypoint and load the resulting contract changes dict.
    let [
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::RelocatableValue(
            contract_state_changes_end,
        ))),
    ] = implicit_return_values.as_slice()
    else {
        panic!("Unexpected implicit return values: {implicit_return_values:?}");
    };
    let actual_contract_changes = parse_contract_changes(
        &runner.vm,
        contract_state_changes.try_into().unwrap(),
        *contract_state_changes_end,
    );

    // Verify the resulting contract changes dict.
    assert_eq!(
        HashSet::from_iter(actual_contract_changes.keys().map(|address| ***address)),
        contract_addresses
    );
    for (contract_address, contract_change) in actual_contract_changes.iter() {
        // Iterate over all storage changes for the contract address and verify that each change is
        // as expected.
        let expected_contract_storage =
            expected_storages.remove(contract_address).unwrap_or_default();
        assert_eq!(contract_change.storage_changes.len(), expected_contract_storage.len());
        for full_contract_change in contract_change.storage_changes.iter() {
            let expected_value = expected_contract_storage.get(&full_contract_change.key).unwrap();
            assert_eq!(full_contract_change.prev_value, Felt::ZERO);
            assert_eq!(full_contract_change.new_value, *expected_value);
            // TODO(Dori): If and when we get access to the final state of the hint processor,
            //   verify that the current state in the execution helper for this contract address
            //   and storage key is as expected.
        }

        // Verify class hashes.
        let expected_class_hash =
            expected_class_hashes.get(contract_address).cloned().unwrap_or(&initial_class_hash);
        assert_eq!(contract_change.prev_class_hash.0, initial_class_hash);
        assert_eq!(contract_change.new_class_hash.0, *expected_class_hash);
    }
}
