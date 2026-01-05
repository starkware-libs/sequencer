use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::{calldata, invoke_tx_args};
use starknet_os::runner::run_virtual_os;
use starknet_types_core::felt::Felt;

use crate::test_manager::TestBuilder;
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;

#[rstest]
#[tokio::test]
async fn test_basic_happy_flow() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    let (mut test_builder, [contract_address]) =
        TestBuilder::create_standard_virtual([(test_contract, calldata![Felt::ONE, Felt::TWO])])
            .await;

    let calldata = create_calldata(contract_address, "test_storage_read", &[Felt::ONE]);
    test_builder.add_funded_account_invoke(
        invoke_tx_args! { calldata, resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS },
    );

    let test_runner = test_builder.build().await;
    // TODO(Yoni): add running and verification logic to test_manager.rs.
    run_virtual_os(test_runner.os_hints).unwrap();
}
