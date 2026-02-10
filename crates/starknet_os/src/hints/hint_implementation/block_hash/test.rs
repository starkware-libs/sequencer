use std::collections::HashMap;

use apollo_starknet_os_program::{OS_PROGRAM, OS_PROGRAM_BYTES};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
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
    gas_prices_to_hash,
    BlockHashVersion,
    BlockHeaderCommitments,
    PartialBlockHashComponents,
    STARKNET_BLOCK_HASH1,
};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    ReceiptCommitment,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::hash::PoseidonHash;
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

fn cairo_calculate_block_hash(
    components: &PartialBlockHashComponents,
    state_root: Felt,
    previous_block_hash: Felt,
) -> Felt {
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::starknet,
        add_main_prefix_to_entrypoint: false,
        ..Default::default()
    };

    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::poseidon)];
    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        OS_PROGRAM_BYTES,
        "starkware.starknet.core.os.block_hash.calculate_block_hash",
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

    let [gas_prices_hash]: [Felt; 1] = gas_prices_to_hash(
        &components.l1_gas_price,
        &components.l1_data_gas_price,
        &components.l2_gas_price,
        &components.starknet_version.try_into().unwrap(),
    )
    .try_into()
    .expect("gas_prices_to_hash should return a single felt");

    let explicit_args = vec![
        block_info_arg,
        header_commitments_arg,
        EndpointArg::from(gas_prices_hash),
        EndpointArg::from(state_root),
        EndpointArg::from(previous_block_hash),
        EndpointArg::from(Felt::try_from(&components.starknet_version).unwrap()),
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
        _ => panic!("Unexpected return value type: {:?}", explicit_return_values[0]),
    }
}

#[rstest]
fn test_block_hash_cairo() {
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
    let previous_block_hash = Felt::from(17);

    let cairo_hash = cairo_calculate_block_hash(&components, state_root, previous_block_hash);

    let expected_hash =
        calculate_block_hash(&components, GlobalRoot(state_root), BlockHash(previous_block_hash))
            .unwrap()
            .0;

    assert_eq!(cairo_hash, expected_hash);
}

#[rstest]
fn test_block_hash_version() {
    let (_, cairo_block_hash_version_felt) =
        OS_PROGRAM.constants.iter().find(|(name, _)| name.ends_with("BLOCK_HASH_VERSION")).unwrap();

    let latest_block_hash_version: Felt =
        BlockHashVersion::try_from(StarknetVersion::LATEST).unwrap().into();

    // NOTE: if these checks fail, it means the block hash version in the OS program is not the
    // latest, and a backward-compatibility flow must be added for the transition.
    assert_eq!(
        *STARKNET_BLOCK_HASH1, latest_block_hash_version,
        "Latest block hash version constant mismatch"
    );
    assert_eq!(
        *cairo_block_hash_version_felt, latest_block_hash_version,
        "Cairo BLOCK_HASH_VERSION constant mismatch"
    );
}
