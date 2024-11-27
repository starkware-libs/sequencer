use rstest::rstest;

use crate::state_reader::utils::reexecute_block_for_testing;

#[rstest]
#[case::v_0_13_0(600001)]
#[case::v_0_13_1(620978)]
#[case::v_0_13_1_1(649367)]
#[case::v_0_13_2(685878)]
#[case::v_0_13_2_1(700000)]
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
    reexecute_block_for_testing(block_number);
}
