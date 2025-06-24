use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::transaction::fields::Calldata;
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::entry_point::CallEntryPoint;
use crate::retdata;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, BALANCE};

#[test_case(RunnableCairo1::Casm; "VM")]
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
fn test_keccak(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_keccak"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let execution = entry_point_call.execute_directly(&mut state).unwrap().execution;
    if runnable_version.is_cairo_native() {
        expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: true,
            failed: false,
            gas_consumed: 236667,
        }
    "#]]
    } else {
        expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: false,
            failed: false,
            gas_consumed: 236667,
        }
    "#]]
    }
    .assert_debug_eq(&execution);
    pretty_assertions::assert_eq!(execution.retdata, retdata![]);
}
