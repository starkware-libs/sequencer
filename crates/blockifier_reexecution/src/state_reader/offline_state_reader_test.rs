use std::collections::HashSet;

use blockifier::blockifier::config::{CairoNativeRunConfig, ContractClassManagerConfig};
use blockifier::state::contract_class_manager::ContractClassManager;
use rstest::rstest;
use starknet_api::block::{BlockNumber, StarknetVersion};
use strum::IntoEnumIterator;

use crate::cli::{get_block_numbers_for_reexecution, REEXECUTION_BLOCK_PER_VERSION};
use crate::state_reader::offline_state_reader::OfflineBlockReexecutor;
use crate::state_reader::reexecution_state_reader::BlockReexecutor;

/// The lowest Starknet version with re-execution coverage; earlier versions predate the
/// offline re-execution files and are not expected to be covered.
const LOWEST_REEXECUTED_VERSION: StarknetVersion = StarknetVersion::V0_13_0;

/// Number of latest versions exempt from coverage: the latest, and the one before it, may not
/// have a mainnet block to re-execute yet.
const NUM_UNCOVERED_LATEST_VERSIONS: usize = 2;

/// Reexecutes a block from a pre-saved JSON file and verifies correctness.
fn reexecute_block_for_testing(block_number: u64) {
    // In tests we are already in the blockifier_reexecution directory.
    let full_file_path = format!("./resources/block_{block_number}/reexecution_data.json");

    // Initialize the contract class manager.
    let mut contract_class_manager_config = ContractClassManagerConfig::default();
    if cfg!(feature = "cairo_native") {
        contract_class_manager_config.cairo_native_run_config =
            CairoNativeRunConfig::wait_on_compilation_for_testing();
    }
    let contract_class_manager = ContractClassManager::start(contract_class_manager_config);

    let matched = OfflineBlockReexecutor::new_from_file(&full_file_path, contract_class_manager)
        .unwrap()
        .reexecute_and_verify_correctness(BlockNumber(block_number));
    assert!(matched, "Reexecution failed for block {block_number}.");
    println!("Reexecution test for block {block_number} passed successfully.");
}

#[rstest]
#[case::v_0_13_0(600001)]
#[case::v_0_13_1(620978)]
#[case::v_0_13_1_1(649367)]
#[case::v_0_13_2(685878)]
#[case::v_0_13_2_1(700000)]
#[case::v_0_13_3(1000000)]
#[case::v_0_13_4(1257000)]
#[case::v_0_13_5(1300000)]
#[case::v_0_13_6(1743490)]
#[case::v_0_14_0(2509604)]
#[case::v_0_14_1(4448394)]
#[case::v_0_14_1_sierra_gas_revert(6481044)]
#[case::v_0_14_2_with_proof_facts(9023035)]
#[case::first_v_0_13_5_rpc_v8(1400000)]
#[case::second_v_0_13_5_rpc_v8(1450000)]
#[case::invoke_with_replace_class_syscall(780008)]
#[case::invoke_with_deploy_syscall(870136)]
#[case::example_deploy_account_v1(837408)]
#[case::example_deploy_account_v3(837792)]
#[case::example_declare_v1(837461)]
#[case::example_declare_v2(822636)]
#[case::example_declare_v3(825013)]
#[case::example_l1_handler(868429)]
#[tokio::test]
#[ignore = "Requires downloading JSON files prior to running; Long test, run with --release flag."]
async fn test_block_reexecution(#[case] block_number: u64) {
    // Assert that the block number is one of the re-execution blocks.
    assert!(get_block_numbers_for_reexecution().contains(&BlockNumber(block_number)));
    tokio::task::spawn_blocking(move || reexecute_block_for_testing(block_number)).await.unwrap();
}

/// The block numbers covered by the `test_block_reexecution` cases above.
/// Keep this in sync with the `#[case]` list; `test_reexecution_cases_match_block_numbers`
/// enforces that it matches [`get_block_numbers_for_reexecution`].
const REEXECUTION_TEST_CASE_BLOCK_NUMBERS: &[u64] = &[
    600001, 620978, 649367, 685878, 700000, 1000000, 1257000, 1300000, 1743490, 2509604, 4448394,
    6481044, 9023035, 1400000, 1450000, 780008, 870136, 837408, 837792, 837461, 822636, 825013,
    868429,
];

/// Every `test_block_reexecution` case must correspond to a re-execution block, and every
/// re-execution block must have a case - otherwise a block would silently go untested.
#[test]
fn test_reexecution_cases_match_block_numbers() {
    let case_block_numbers: HashSet<u64> =
        REEXECUTION_TEST_CASE_BLOCK_NUMBERS.iter().copied().collect();
    let reexecution_block_numbers: HashSet<u64> =
        get_block_numbers_for_reexecution().iter().map(|block_number| block_number.0).collect();
    assert_eq!(
        case_block_numbers, reexecution_block_numbers,
        "test_block_reexecution cases drifted from the re-execution block list. Update the \
         #[case] list (and REEXECUTION_TEST_CASE_BLOCK_NUMBERS) to match \
         get_block_numbers_for_reexecution."
    );
}

/// Every Starknet version (from the lowest re-executed version up to the latest two, which are
/// exempt) must have a re-execution block in `test_block_reexecution`.
#[test]
fn test_all_starknet_versions_are_reexecuted() {
    let all_versions: Vec<StarknetVersion> = StarknetVersion::iter().collect();
    // `StarknetVersion::iter` yields variants in ascending order, so the last entries are the
    // latest versions.
    let uncovered_latest_versions: HashSet<StarknetVersion> = all_versions
        [all_versions.len() - NUM_UNCOVERED_LATEST_VERSIONS..]
        .iter()
        .copied()
        .collect();
    let covered_versions: HashSet<StarknetVersion> =
        REEXECUTION_BLOCK_PER_VERSION.iter().map(|(version, _block_number)| *version).collect();

    let missing_versions: Vec<StarknetVersion> = all_versions
        .iter()
        .copied()
        .filter(|version| {
            *version >= LOWEST_REEXECUTED_VERSION
                && !uncovered_latest_versions.contains(version)
                && !covered_versions.contains(version)
        })
        .collect();

    assert!(
        missing_versions.is_empty(),
        "Starknet versions missing a re-execution block: {missing_versions:?}. Add a block for \
         each to REEXECUTION_BLOCK_PER_VERSION (see the blockifier_reexecution README)."
    );
}
