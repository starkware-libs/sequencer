use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::{ascii_as_felt, ChainId, Nonce};
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::deploy_account::deploy_account_tx;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::fields::{
    valid_resource_bounds_as_felts,
    Calldata,
    ContractAddressSalt,
    ProofFacts,
    ResourceAsFelts,
    Tip,
    ValidResourceBounds,
};
use starknet_api::transaction::{
    CalculateContractAddress,
    L1HandlerTransaction,
    TransactionHasher,
    TransactionVersion,
};
use starknet_api::{
    calldata,
    class_hash,
    compiled_class_hash,
    contract_address,
    declare_tx_args,
    deploy_account_tx_args,
    felt,
    invoke_tx_args,
    nonce,
    proof_facts,
};
use starknet_types_core::felt::Felt;

use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

const TRANSACTION_HASH_MODULE: &str =
    "starkware.starknet.core.os.transaction_hash.transaction_hash";

fn poseidon_implicit_args() -> Vec<ImplicitArg> {
    vec![
        ImplicitArg::Builtin(BuiltinName::range_check),
        ImplicitArg::Builtin(BuiltinName::poseidon),
    ]
}

/// Wraps a felt slice as a Cairo `felt*` pointer argument.
fn felt_array_arg(felts: &[Felt]) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Array(
        felts.iter().copied().map(MaybeRelocatable::from).collect(),
    ))
}

/// Builds the Cairo `CommonTxFields*` argument positionally. The element order MUST mirror the
/// `CommonTxFields` struct in `transaction_hash.cairo`; the OS reads and asserts on every field, so
/// the fixed members (`n_resource_bounds = 3`, empty paymaster data, L1 DA modes) are set here.
fn common_tx_fields_arg(
    tx_hash_prefix: Felt,
    sender_address: Felt,
    chain_id: Felt,
    nonce: Felt,
    tip: Felt,
    resource_bounds: &ValidResourceBounds,
) -> EndpointArg {
    let resource_felts = valid_resource_bounds_as_felts(resource_bounds, false)
        .unwrap()
        .into_iter()
        .flat_map(ResourceAsFelts::flatten)
        .map(MaybeRelocatable::from)
        .collect();
    EndpointArg::Pointer(PointerArg::Composed(vec![
        EndpointArg::from(tx_hash_prefix),
        EndpointArg::from(TransactionVersion::THREE.0),
        EndpointArg::from(sender_address),
        EndpointArg::from(chain_id),
        EndpointArg::from(nonce),
        EndpointArg::from(tip),
        EndpointArg::from(3), // n_resource_bounds
        EndpointArg::Pointer(PointerArg::Array(resource_felts)),
        EndpointArg::from(0),                            // paymaster_data_length
        EndpointArg::Pointer(PointerArg::Array(vec![])), // paymaster_data
        EndpointArg::from(0),                            // nonce_data_availability_mode
        EndpointArg::from(0),                            // fee_data_availability_mode
    ]))
}

/// Builds an `ExecutionContext*` argument positionally (element order mirrors `ExecutionContext` in
/// `execute_entry_point.cairo`). `execution_info` is supplied by the caller; the members the tx
/// hashers never read are zeroed.
fn execution_context_arg(calldata: &[Felt], execution_info: EndpointArg) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Composed(vec![
        EndpointArg::from(0),              // entry_point_type
        EndpointArg::from(0),              // class_hash
        EndpointArg::from(calldata.len()), // calldata_size
        felt_array_arg(calldata),
        execution_info,
        EndpointArg::from(0), // deprecated_tx_info
    ]))
}

/// Builds an `ExecutionInfo*` argument positionally (element order mirrors `ExecutionInfo` in
/// `new_syscalls.cairo`). Only `contract_address` and `selector` are read by the L1-handler hasher.
fn execution_info_arg(contract_address: Felt, selector: Felt) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Composed(vec![
        EndpointArg::from(0), // block_info
        EndpointArg::from(0), // tx_info
        EndpointArg::from(0), // caller_address
        EndpointArg::from(contract_address),
        EndpointArg::from(selector),
    ]))
}

/// Runs the given OS transaction-hash entrypoint and extracts the single returned felt.
fn run_os_tx_hasher(
    entrypoint_name: &str,
    explicit_args: Vec<EndpointArg>,
    implicit_args: Vec<ImplicitArg>,
) -> Felt {
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::starknet,
        add_main_prefix_to_entrypoint: false,
        ..Default::default()
    };
    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        OS_PROGRAM_BYTES,
        &format!("{TRANSACTION_HASH_MODULE}.{entrypoint_name}"),
        &implicit_args,
        HashMap::new(),
    )
    .expect("Failed to initialize cairo runner.");

    // The entrypoint returns a single felt (the transaction hash).
    let (_, explicit_return_values, _) = run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        None,
        &mut runner,
        &program,
        &runner_config,
        &[EndpointArg::from(0)],
    )
    .expect("Failed to run cairo entrypoint.");

    match &explicit_return_values[0] {
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(felt_value))) => *felt_value,
        other => panic!("Unexpected return value type: {other:?}"),
    }
}

/// Asserts the Rust `starknet_api` deploy-account V3 hasher agrees with the OS Cairo hasher. The
/// non-zero nonce cases are the direct regression guard for the `data_availability_mode`/`nonce`
/// ordering bug.
#[rstest]
#[case::nonzero_nonce_empty_calldata(nonce!(7_u64), calldata![], Tip(0))]
#[case::nonzero_nonce_multi_calldata(
    nonce!(42_u64),
    calldata![felt!(11_u64), felt!(22_u64), felt!(33_u64)],
    Tip(99)
)]
#[case::nonzero_nonce_single_calldata(nonce!(0x1234_5678_u64), calldata![felt!(5_u64)], Tip(7))]
fn test_deploy_account_v3_hash_consistency(
    #[case] nonce: Nonce,
    #[case] constructor_calldata: Calldata,
    #[case] tip: Tip,
) {
    let chain_id = ChainId::Other("SN_CONSISTENCY_TEST".to_string());
    let class_hash = class_hash!(0x1234_u64);
    let contract_address_salt = ContractAddressSalt(felt!(0xabcd_u64));

    let tx = deploy_account_tx(
        deploy_account_tx_args! {
            class_hash,
            contract_address_salt,
            constructor_calldata: constructor_calldata.clone(),
            tip,
        },
        nonce,
    );

    let rust_hash = tx.calculate_transaction_hash(&chain_id, &TransactionVersion::THREE).unwrap().0;
    let sender_address = tx.calculate_contract_address().unwrap();

    // The OS calldata is [class_hash, salt, ...constructor_calldata].
    let mut os_calldata = vec![class_hash.0, contract_address_salt.0];
    os_calldata.extend(constructor_calldata.0.iter().copied());

    let common_fields = common_tx_fields_arg(
        ascii_as_felt("deploy_account").unwrap(),
        *sender_address.0.key(),
        Felt::try_from(&chain_id).unwrap(),
        nonce.0,
        Felt::from(tip.0),
        &tx.resource_bounds(),
    );
    let cairo_hash = run_os_tx_hasher(
        "compute_deploy_account_transaction_hash",
        vec![common_fields, EndpointArg::from(os_calldata.len()), felt_array_arg(&os_calldata)],
        poseidon_implicit_args(),
    );

    assert_eq!(rust_hash, cairo_hash);
}

/// Asserts the Rust `starknet_api` declare V3 hasher agrees with the OS Cairo hasher, varying
/// nonce, tip, and the class / compiled-class hashes.
#[rstest]
#[case::nonzero_nonce(nonce!(9_u64), Tip(0))]
#[case::large_tip(nonce!(0xdead_u64), Tip(123_456))]
fn test_declare_v3_hash_consistency(#[case] nonce: Nonce, #[case] tip: Tip) {
    let chain_id = ChainId::Other("SN_CONSISTENCY_TEST".to_string());
    let sender_address = contract_address!(0x1111_u64);
    let class_hash = class_hash!(0x2222_u64);
    let compiled_class_hash = compiled_class_hash!(0x3333_u64);

    let tx = declare_tx(declare_tx_args! {
        sender_address,
        class_hash,
        compiled_class_hash,
        nonce,
        tip,
    });

    let rust_hash = tx.calculate_transaction_hash(&chain_id, &TransactionVersion::THREE).unwrap().0;

    let common_fields = common_tx_fields_arg(
        ascii_as_felt("declare").unwrap(),
        *sender_address.0.key(),
        Felt::try_from(&chain_id).unwrap(),
        nonce.0,
        Felt::from(tip.0),
        &tx.resource_bounds(),
    );
    let cairo_hash = run_os_tx_hasher(
        "compute_declare_transaction_hash",
        vec![
            common_fields,
            EndpointArg::from(class_hash.0),
            EndpointArg::from(compiled_class_hash.0),
            EndpointArg::from(0), // account_deployment_data_size (OS asserts == 0)
            felt_array_arg(&[]),  // account_deployment_data
        ],
        poseidon_implicit_args(),
    );

    assert_eq!(rust_hash, cairo_hash);
}

/// Asserts the Rust `starknet_api` invoke V3 hasher agrees with the OS Cairo hasher, covering
/// empty/single/multi calldata and the optional proof-facts tail (empty vs non-empty), which
/// exercises the OS's backward-compatibility branch.
#[rstest]
#[case::empty_calldata_no_proof_facts(calldata![], proof_facts![])]
#[case::single_calldata_no_proof_facts(calldata![felt!(7_u64)], proof_facts![])]
#[case::multi_calldata_no_proof_facts(calldata![felt!(1_u64), felt!(2_u64), felt!(3_u64)], proof_facts![])]
#[case::multi_calldata_with_proof_facts(
    calldata![felt!(8_u64), felt!(9_u64)],
    proof_facts![felt!(111_u64), felt!(222_u64)]
)]
fn test_invoke_v3_hash_consistency(#[case] calldata: Calldata, #[case] proof_facts: ProofFacts) {
    let chain_id = ChainId::Other("SN_CONSISTENCY_TEST".to_string());
    let sender_address = contract_address!(0x4321_u64);
    let nonce = nonce!(55_u64);
    let tip = Tip(77);

    let tx = invoke_tx(invoke_tx_args! {
        sender_address,
        calldata: calldata.clone(),
        nonce,
        tip,
        proof_facts: proof_facts.clone(),
    });

    let rust_hash = tx.calculate_transaction_hash(&chain_id, &TransactionVersion::THREE).unwrap().0;

    let common_fields = common_tx_fields_arg(
        ascii_as_felt("invoke").unwrap(),
        *sender_address.0.key(),
        Felt::try_from(&chain_id).unwrap(),
        nonce.0,
        Felt::from(tip.0),
        &tx.resource_bounds(),
    );
    // The invoke hasher reads only `calldata` from the execution context, so `execution_info` is
    // left null.
    let cairo_hash = run_os_tx_hasher(
        "compute_invoke_transaction_hash",
        vec![
            common_fields,
            execution_context_arg(&calldata.0, EndpointArg::from(0)),
            EndpointArg::from(0), // account_deployment_data_size (OS asserts == 0)
            felt_array_arg(&[]),  // account_deployment_data
            EndpointArg::from(proof_facts.0.len()),
            felt_array_arg(&proof_facts.0),
        ],
        poseidon_implicit_args(),
    );

    assert_eq!(rust_hash, cairo_hash);
}

/// Asserts the Rust `starknet_api` L1-handler hasher agrees with the OS Cairo hasher. This
/// validates the Pedersen hash path independently of the Poseidon V3 path, varying nonce and
/// calldata length.
#[rstest]
#[case::empty_calldata(nonce!(3_u64), calldata![])]
#[case::single_calldata(nonce!(0xfeed_u64), calldata![felt!(42_u64)])]
#[case::multi_calldata(nonce!(88_u64), calldata![felt!(1_u64), felt!(2_u64), felt!(3_u64)])]
fn test_l1_handler_hash_consistency(#[case] nonce: Nonce, #[case] calldata: Calldata) {
    let chain_id = ChainId::Other("SN_CONSISTENCY_TEST".to_string());
    let contract_address = contract_address!(0x9999_u64);
    let entry_point_selector = selector_from_name("l1_handler_entry_point");

    let tx = L1HandlerTransaction {
        version: L1HandlerTransaction::VERSION,
        nonce,
        contract_address,
        entry_point_selector,
        calldata: calldata.clone(),
    };

    let rust_hash =
        tx.calculate_transaction_hash(&chain_id, &L1HandlerTransaction::VERSION).unwrap().0;

    let execution_context = execution_context_arg(
        &calldata.0,
        execution_info_arg(*contract_address.0.key(), entry_point_selector.0),
    );
    let cairo_hash = run_os_tx_hasher(
        "compute_l1_handler_transaction_hash",
        vec![
            execution_context,
            EndpointArg::from(Felt::try_from(&chain_id).unwrap()),
            EndpointArg::from(nonce.0),
        ],
        vec![ImplicitArg::Builtin(BuiltinName::pedersen)],
    );

    assert_eq!(rust_hash, cairo_hash);
}
