use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::contract_class::SierraVersion;
use starknet_api::transaction::fields::Calldata;

use crate::context::{BlockContext, ChainInfo};
use crate::execution::entry_point::CallEntryPoint;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, CairoVersion, RunnableCairo1, BALANCE};

#[rstest]
#[cfg_attr(
    feature = "cairo_native",
    case::pedersen_native(RunnableCairo1::Native, "test_pedersen")
)]
#[cfg_attr(feature = "cairo_native", case::bitwise_native(RunnableCairo1::Native, "test_bitwise"))]
#[cfg_attr(feature = "cairo_native", case::ecop_native(RunnableCairo1::Native, "test_ecop"))]
#[cfg_attr(
    feature = "cairo_native",
    case::poseidon_native(RunnableCairo1::Native, "test_poseidon")
)]
#[cfg_attr(
    feature = "cairo_native",
    case::add_and_mul_mod_native(RunnableCairo1::Native, "test_add_and_mul_mod")
)]
#[case::pedersen_vm(RunnableCairo1::Casm, "test_pedersen")]
#[case::bitwise_vm(RunnableCairo1::Casm, "test_bitwise")]
#[case::ecop_vm(RunnableCairo1::Casm, "test_ecop")]
#[case::poseidon_vm(RunnableCairo1::Casm, "test_poseidon")]
#[case::add_and_mul_mod_vm(RunnableCairo1::Casm, "test_add_and_mul_mod")]
fn builtins_test(#[case] runnable_version: RunnableCairo1, #[case] selector_name: &str) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name(selector_name),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let call_info_while_tracking_gas_consumed =
        entry_point_call.clone().execute_directly(&mut state).unwrap();

    let mut block_context = BlockContext::create_for_account_testing();
    block_context.versioned_constants.min_sierra_version_for_sierra_gas =
        SierraVersion::new(2, 8, 0);

    let call_info_while_tracking_vm_resources =
        entry_point_call.execute_directly(&mut state).unwrap();

    pretty_assertions::assert_eq!(
        call_info_while_tracking_vm_resources.execution,
        call_info_while_tracking_gas_consumed.execution
    );
}
