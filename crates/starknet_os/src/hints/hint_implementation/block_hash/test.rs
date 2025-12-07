use std::collections::HashMap;
use std::sync::LazyLock;

use apollo_infra_utils::cairo0_compiler::compile_cairo0_program;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::{fixture, rstest};
use starknet_api::block::{
    BlockHash,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_hash,
    BlockHashVersion,
    BlockHeaderCommitments,
    PartialBlockHashComponents,
};
use starknet_api::core::{
    ascii_as_felt,
    EventCommitment,
    GlobalRoot,
    ReceiptCommitment,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::hash::PoseidonHash;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash as CoreStarkHash};

use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

// TODO(Yoni): use the OS program bytes instead once the block hash is reachable by the OS.
static BLOCK_HASH_PROGRAM: LazyLock<(Vec<u8>, Program)> = LazyLock::new(|| {
    let cairo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/apollo_starknet_os_program/src/cairo")
        .canonicalize()
        .unwrap();
    let block_hash_file = cairo_root.join("starkware/starknet/core/os/block_hash.cairo");
    let program_bytes = compile_cairo0_program(block_hash_file, cairo_root)
        .expect("Failed to compile cairo0 program");
    let program = cairo_vm::types::program::Program::from_bytes(&program_bytes, None)
        .expect("Failed to load program");

    (program_bytes, program)
});

#[fixture]
fn block_hash_program() -> (Vec<u8>, Program) {
    BLOCK_HASH_PROGRAM.clone()
}

fn cairo_calculate_block_hash(
    program_bytes: &[u8],
    components: &PartialBlockHashComponents,
    state_root: Felt,
    parent_hash: Felt,
) -> Felt {
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::starknet,
        add_main_prefix_to_entrypoint: true, // It's the main file now
        ..Default::default()
    };

    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::poseidon)];
    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        program_bytes,
        "calculate_block_hash",
        &implicit_args,
        HashMap::new(),
    )
    .expect("Failed to initialize cairo runner");

    let block_info_arg = EndpointArg::Pointer(PointerArg::Array(vec![
        Felt::from(components.block_number.0).into(),
        Felt::from(components.timestamp.0).into(),
        components.sequencer.0.0.key().into(),
    ]));

    let header_commitments_arg = EndpointArg::Pointer(PointerArg::Array(vec![
        components.header_commitments.transaction_commitment.0.into(),
        components.header_commitments.event_commitment.0.into(),
        components.header_commitments.receipt_commitment.0.into(),
        components.header_commitments.state_diff_commitment.0.0.into(),
        components.header_commitments.concatenated_counts.into(),
    ]));

    let gas_prices_hash = Poseidon::hash_array(&[
        ascii_as_felt("STARKNET_GAS_PRICES0").unwrap(),
        components.l1_gas_price.price_in_wei.0.into(),
        components.l1_gas_price.price_in_fri.0.into(),
        components.l1_data_gas_price.price_in_wei.0.into(),
        components.l1_data_gas_price.price_in_fri.0.into(),
        components.l2_gas_price.price_in_wei.0.into(),
        components.l2_gas_price.price_in_fri.0.into(),
    ]);

    let explicit_args = vec![
        block_info_arg,
        header_commitments_arg,
        EndpointArg::from(gas_prices_hash),
        EndpointArg::from(state_root),
        EndpointArg::from(parent_hash),
        EndpointArg::from(ascii_as_felt(&components.starknet_version.to_string()).unwrap()),
    ];

    // We expect one felt as return value (block_hash).
    let expected_explicit_return_values = vec![EndpointArg::from(0)];

    let (_, explicit_return_values, _) = run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        None,
        &mut runner,
        &program,
        &runner_config,
        &expected_explicit_return_values,
    )
    .expect("Failed to run cairo entrypoint");

    match &explicit_return_values[0] {
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(val))) => *val,
        _ => panic!("Unexpected return value type"),
    }
}

#[rstest]
fn test_block_hash_cairo(block_hash_program: (Vec<u8>, Program)) {
    let (program_bytes, _) = block_hash_program;
    let components = PartialBlockHashComponents {
        block_number: BlockNumber(1),
        timestamp: BlockTimestamp(2),
        sequencer: SequencerContractAddress(Felt::from(3).try_into().unwrap()),
        header_commitments: BlockHeaderCommitments {
            transaction_commitment: TransactionCommitment(Felt::from(4)),
            event_commitment: EventCommitment(Felt::from(5)),
            receipt_commitment: ReceiptCommitment(Felt::from(6)),
            state_diff_commitment: StateDiffCommitment(PoseidonHash(Felt::from(7))),
            concatenated_counts: Felt::from(8),
        },
        l1_gas_price: GasPricePerToken { price_in_wei: GasPrice(10), price_in_fri: GasPrice(11) },
        l1_data_gas_price: GasPricePerToken {
            price_in_wei: GasPrice(12),
            price_in_fri: GasPrice(13),
        },
        l2_gas_price: GasPricePerToken { price_in_wei: GasPrice(14), price_in_fri: GasPrice(15) },
        starknet_version: StarknetVersion::LATEST,
    };
    let state_root = Felt::from(16);
    let parent_hash = Felt::from(17);

    let cairo_hash =
        cairo_calculate_block_hash(&program_bytes, &components, state_root, parent_hash);

    let expected_hash =
        calculate_block_hash(&components, GlobalRoot(state_root), BlockHash(parent_hash))
            .unwrap()
            .0;

    assert_eq!(cairo_hash, expected_hash);
}

#[rstest]
fn test_block_hash_version(block_hash_program: (Vec<u8>, Program)) {
    let (_, program) = block_hash_program;
    let (_, cairo_block_hash_version_felt) =
        program.constants.iter().find(|(name, _)| name.ends_with("BLOCK_HASH_VERSION")).unwrap();

    let latest_block_hash_version = BlockHashVersion::try_from(StarknetVersion::LATEST).unwrap();

    // NOTE: if this check fails, it means the block hash version in the OS program is not the
    // latest, and a backward-compatibility flow must be added for the transition.
    assert_eq!(
        *cairo_block_hash_version_felt,
        latest_block_hash_version.into(),
        "Cairo BLOCK_HASH_VERSION constant mismatch"
    );
}
