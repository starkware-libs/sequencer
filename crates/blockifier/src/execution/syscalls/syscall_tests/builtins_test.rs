use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::transaction::fields::Calldata;

use crate::context::ChainInfo;
use crate::execution::call_info::CallExecution;
use crate::execution::entry_point::CallEntryPoint;
use crate::retdata;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, CairoVersion, RunnableCairo1, BALANCE};

#[rstest]
#[cfg_attr(
    feature = "cairo_native",
    case::pedersen_native(RunnableCairo1::Native, "test_pedersen", 7190)
)]
#[cfg_attr(
    feature = "cairo_native",
    case::bitwise_native(RunnableCairo1::Native, "test_bitwise", 3723)
)]
#[cfg_attr(feature = "cairo_native", case::ecop_native(RunnableCairo1::Native, "test_ecop", 9425))]
#[cfg_attr(
    feature = "cairo_native",
    case::poseidon_native(RunnableCairo1::Native, "test_poseidon", 5431)
)]
#[cfg_attr(
    feature = "cairo_native",
    case::add_and_mul_mod_native(RunnableCairo1::Native, "test_add_and_mul_mod", 21724)
)]
#[case::pedersen_vm(RunnableCairo1::Casm, "test_pedersen", 7190)]
#[case::bitwise_vm(RunnableCairo1::Casm, "test_bitwise", 3723)]
#[case::ecop_vm(RunnableCairo1::Casm, "test_ecop", 9425)]
#[case::poseidon_vm(RunnableCairo1::Casm, "test_poseidon", 5431)]
#[case::add_and_mul_mod_vm(RunnableCairo1::Casm, "test_add_and_mul_mod", 21724)]
fn builtins_test(
    #[case] runnable_version: RunnableCairo1,
    #[case] selector_name: &str,
    #[case] gas_consumed: u64,
) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name(selector_name),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed, ..CallExecution::from_retdata(retdata![]) }
    );
}
