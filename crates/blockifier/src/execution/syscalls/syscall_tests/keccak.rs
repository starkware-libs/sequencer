use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::transaction::fields::Calldata;
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::call_info::CallExecution;
use crate::execution::entry_point::CallEntryPoint;
use crate::retdata;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{
    trivial_external_entry_point_new,
    CairoVersion,
    RunnableCairoVersion,
    BALANCE,
};

#[test_case(CairoVersion::Cairo1(RunnableCairoVersion::Casm); "VM")]
#[cfg_attr(feature = "cairo_native", test_case(CairoVersion::Cairo1(RunnableCairoVersion::Native); "Native"))]
fn test_keccak(cairo_version: CairoVersion) {
    let test_contract = FeatureContract::TestContract(cairo_version);
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_keccak"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed: 254910, ..CallExecution::from_retdata(retdata![]) }
    );
}
