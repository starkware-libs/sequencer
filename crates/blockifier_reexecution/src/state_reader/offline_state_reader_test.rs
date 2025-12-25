use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use rstest::rstest;
use starknet_api::block::BlockNumber;

use crate::cli::get_block_numbers_for_reexecution;
use crate::state_reader::offline_state_reader::OfflineConsecutiveStateReaders;
use crate::state_reader::reexecution_state_reader::ConsecutiveReexecutionStateReaders;

/// Reexecutes a block from a pre-saved JSON file and verifies correctness.
fn reexecute_block_for_testing(block_number: u64) {
    // In tests we are already in the blockifier_reexecution directory.
    let full_file_path = format!("./resources/block_{block_number}/reexecution_data.json");

    // Initialize the contract class manager.
    let mut contract_class_manager_config = ContractClassManagerConfig::default();
    if cfg!(feature = "cairo_native") {
        contract_class_manager_config.cairo_native_run_config.wait_on_native_compilation = true;
        contract_class_manager_config.cairo_native_run_config.run_cairo_native = true;
    }
    let contract_class_manager = ContractClassManager::start(contract_class_manager_config);

    OfflineConsecutiveStateReaders::new_from_file(&full_file_path, contract_class_manager)
        .unwrap()
        .reexecute_and_verify_correctness();

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
#[ignore = "Requires downloading JSON files prior to running; Long test, run with --release flag."]
fn test_block_reexecution(#[case] block_number: u64) {
    // Assert that the block number exists in the json file.
    assert!(
        get_block_numbers_for_reexecution(Some("../../".to_owned()))
            .contains(&BlockNumber(block_number))
    );
    reexecute_block_for_testing(block_number);
}
