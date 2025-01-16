use semver::Version;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::transaction::fields::Calldata;
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::call_info::CallExecution;
use crate::execution::entry_point::CallEntryPoint;
use crate::retdata;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, CairoVersion, RunnableCairo1, BALANCE};
use crate::versioned_constants::VersionedConstants;

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_range_check(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_range_check"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let gas_costs = &VersionedConstants::create_for_testing().os_constants.gas_costs;
    let expected_gas = 27*gas_costs.builtins.range_check + 108*gas_costs.base.step_gas_cost + 2240;
    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed: expected_gas, ..CallExecution::from_retdata(retdata![]) }
    );
}


#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_pedersen(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_pedersen"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let gas_costs = &VersionedConstants::create_for_testing().os_constants.gas_costs;
    let expected_gas = gas_costs.builtins.pedersen + gas_costs.base.syscall_base_gas_cost ;
    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed: expected_gas, ..CallExecution::from_retdata(retdata![]) }
    );
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_bitwise(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_bitwise"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let gas_costs = &VersionedConstants::create_for_testing().os_constants.gas_costs;
    let expected_gas = gas_costs.builtins.bitwise + gas_costs.base.syscall_base_gas_cost ;
    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed: expected_gas, ..CallExecution::from_retdata(retdata![]) }
    );
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_ecop(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_ecop"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let gas_costs = &VersionedConstants::create_for_testing().os_constants.gas_costs;
    let expected_gas = gas_costs.builtins.ecop + gas_costs.base.syscall_base_gas_cost ;
    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed: expected_gas, ..CallExecution::from_retdata(retdata![]) }
    );
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_poseidon(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_poseidon"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let gas_costs = &VersionedConstants::create_for_testing().os_constants.gas_costs;
    let expected_gas = 2*gas_costs.builtins.poseidon + gas_costs.base.syscall_base_gas_cost ;
    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed: expected_gas, ..CallExecution::from_retdata(retdata![]) }
    );
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_add_mod(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_add_mod"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed: 882425, ..CallExecution::from_retdata(retdata![]) }
    );
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_mul_mod(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_mul_mod"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed: 882425, ..CallExecution::from_retdata(retdata![]) }
    );
}
