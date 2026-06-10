use std::collections::HashMap;
use std::sync::Arc;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::runners::cairo_runner::CairoRunner;
use cairo_vm::vm::vm_core::VirtualMachine;
use rstest::rstest;
use starknet_api::block::GasPrice;
use starknet_api::core::{ascii_as_felt, ChainId, ClassHash, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::{
    CalculateContractAddress,
    DeployAccountTransactionV3,
    TransactionHasher,
    TransactionVersion,
};
use starknet_types_core::felt::Felt;

use crate::hints::vars::CairoStruct;
use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};
use crate::vm_utils::{insert_values_to_fields, LoadCairoObject};

const TRANSACTION_HASH_MODULE: &str =
    "starkware.starknet.core.os.transaction_hash.transaction_hash";

fn os_tx_hasher_runner_config() -> EntryPointRunnerConfig {
    EntryPointRunnerConfig {
        layout: LayoutName::starknet,
        add_main_prefix_to_entrypoint: false,
        ..Default::default()
    }
}

/// Builds the Cairo `CommonTxFields` struct in a fresh VM segment and returns its base pointer.
///
/// The resource bounds are laid out as the three `ResourceBounds` structs (L1 gas, L2 gas, L1 data
/// gas, in that order) the OS asserts on in `hash_fee_fields`. Paymaster data is empty and the data
/// availability modes are zero, matching the OS asserts.
#[allow(clippy::too_many_arguments)]
fn load_common_tx_fields(
    vm: &mut VirtualMachine,
    program: &Program,
    tx_hash_prefix: Felt,
    sender_address: Felt,
    chain_id: Felt,
    nonce: Felt,
    tip: Felt,
    resource_bounds: &ValidResourceBounds,
) -> Relocatable {
    let constants = HashMap::new();

    let resource_bounds_base = vm.add_memory_segment();
    resource_bounds
        .load_into(vm, program, resource_bounds_base, &constants)
        .expect("Failed to load resource bounds.");

    // The paymaster data is empty, but the pointer must reference a valid segment.
    let paymaster_data_base = vm.add_memory_segment();

    let common_fields_base = vm.add_memory_segment();
    insert_values_to_fields(
        common_fields_base,
        CairoStruct::CommonTxFields,
        vm,
        &[
            ("tx_hash_prefix", tx_hash_prefix.into()),
            ("version", TransactionVersion::THREE.0.into()),
            ("sender_address", sender_address.into()),
            ("chain_id", chain_id.into()),
            ("nonce", nonce.into()),
            ("tip", tip.into()),
            ("n_resource_bounds", Felt::THREE.into()),
            ("resource_bounds", resource_bounds_base.into()),
            ("paymaster_data_length", Felt::ZERO.into()),
            ("paymaster_data", paymaster_data_base.into()),
            ("nonce_data_availability_mode", Felt::ZERO.into()),
            ("fee_data_availability_mode", Felt::ZERO.into()),
        ],
        program,
    )
    .expect("Failed to load CommonTxFields.");

    common_fields_base
}

/// Runs the given OS transaction-hash entrypoint and extracts the single returned felt.
fn run_os_tx_hasher(
    runner_config: &EntryPointRunnerConfig,
    runner: &mut CairoRunner,
    program: &Program,
    entrypoint: String,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
) -> Felt {
    // The entrypoint returns a single felt (the transaction hash).
    let expected_explicit_return_values = vec![EndpointArg::from(0)];
    let (_, explicit_return_values, _) = run_cairo_0_entrypoint(
        entrypoint,
        explicit_args,
        implicit_args,
        None,
        runner,
        program,
        runner_config,
        &expected_explicit_return_values,
    )
    .expect("Failed to run cairo entrypoint.");

    match &explicit_return_values[0] {
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(felt_value))) => *felt_value,
        other => panic!("Unexpected return value type: {other:?}"),
    }
}

/// Computes the deploy-account V3 transaction hash via the OS Cairo hasher.
fn cairo_deploy_account_v3_hash(
    sender_address: Felt,
    chain_id: Felt,
    nonce: Felt,
    tip: Felt,
    resource_bounds: &ValidResourceBounds,
    calldata: &[Felt],
) -> Felt {
    let runner_config = os_tx_hasher_runner_config();
    let implicit_args = vec![
        ImplicitArg::Builtin(BuiltinName::range_check),
        ImplicitArg::Builtin(BuiltinName::poseidon),
    ];
    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        OS_PROGRAM_BYTES,
        &format!("{TRANSACTION_HASH_MODULE}.compute_deploy_account_transaction_hash"),
        &implicit_args,
        HashMap::new(),
    )
    .expect("Failed to initialize cairo runner.");

    let common_fields_ptr = load_common_tx_fields(
        &mut runner.vm,
        &program,
        ascii_as_felt("deploy_account").unwrap(),
        sender_address,
        chain_id,
        nonce,
        tip,
        resource_bounds,
    );

    let explicit_args = vec![
        EndpointArg::Value(ValueArg::Single(common_fields_ptr.into())),
        EndpointArg::from(calldata.len()),
        EndpointArg::Pointer(PointerArg::Array(
            calldata.iter().map(|felt| (*felt).into()).collect(),
        )),
    ];

    run_os_tx_hasher(
        &runner_config,
        &mut runner,
        &program,
        entrypoint,
        &explicit_args,
        &implicit_args,
    )
}

fn all_resource_bounds(
    l1_gas: (u64, u128),
    l2_gas: (u64, u128),
    l1_data_gas: (u64, u128),
) -> ValidResourceBounds {
    let make = |(max_amount, max_price_per_unit): (u64, u128)| ResourceBounds {
        max_amount: GasAmount(max_amount),
        max_price_per_unit: GasPrice(max_price_per_unit),
    };
    ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: make(l1_gas),
        l2_gas: make(l2_gas),
        l1_data_gas: make(l1_data_gas),
    })
}

/// Asserts the Rust `starknet_api` deploy-account V3 hasher agrees with the OS Cairo hasher on
/// non-trivial inputs. The non-zero nonce cases are the direct regression guard for the
/// `data_availability_mode`/`nonce` ordering bug.
#[rstest]
#[case::nonzero_nonce_empty_calldata(Nonce(Felt::from(7)), vec![], Tip(0))]
#[case::nonzero_nonce_multi_calldata(
    Nonce(Felt::from(42)),
    vec![Felt::from(11), Felt::from(22), Felt::from(33)],
    Tip(99)
)]
#[case::nonzero_nonce_single_calldata(Nonce(Felt::from(0x1234_5678_u64)), vec![Felt::from(5)], Tip(7))]
fn test_deploy_account_v3_hash_consistency(
    #[case] nonce: Nonce,
    #[case] constructor_calldata: Vec<Felt>,
    #[case] tip: Tip,
) {
    let chain_id = ChainId::Other("SN_CONSISTENCY_TEST".to_string());
    let class_hash = ClassHash(Felt::from(0x1234_u64));
    let contract_address_salt = ContractAddressSalt(Felt::from(0xabcd_u64));
    // Use near-2^64-1 amounts and large prices to exercise the resource-bounds packing.
    let resource_bounds = all_resource_bounds(
        (u64::MAX - 1, u128::from(u64::MAX)),
        (1_000_000, 2_000_000),
        (12_345, 67_890),
    );

    let tx = DeployAccountTransactionV3 {
        resource_bounds,
        tip,
        signature: TransactionSignature::default(),
        nonce,
        class_hash,
        contract_address_salt,
        constructor_calldata: Calldata(Arc::new(constructor_calldata.clone())),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: PaymasterData::default(),
    };

    let rust_hash = tx.calculate_transaction_hash(&chain_id, &TransactionVersion::THREE).unwrap().0;

    let sender_address = tx.calculate_contract_address().unwrap();
    // The OS calldata is [class_hash, salt, ...constructor_calldata].
    let mut calldata = vec![class_hash.0, contract_address_salt.0];
    calldata.extend(constructor_calldata.iter().copied());

    let cairo_hash = cairo_deploy_account_v3_hash(
        *sender_address.0.key(),
        Felt::try_from(&chain_id).unwrap(),
        nonce.0,
        Felt::from(tip.0),
        &resource_bounds,
        &calldata,
    );

    assert_eq!(rust_hash, cairo_hash);
}
