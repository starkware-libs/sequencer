use starknet_api::execution_resources::GasAmount;

use super::to_gas_for_fee;
use crate::execution::call_info::{CallExecution, CallInfo, ChargedResources};
use crate::execution::contract_class::TrackedResource;

#[test]
/// Verifies that every call from the inner most to the outer has the expected gas_for_fee for the
/// following topology (marked as TrackedResource(gas_consumed)):
//       Gas(8) -> Gas(3) -> VM(2) -> VM(1)
//            \ -> VM(4)
// Expected values are 2 -> 1 -> 0 -> 0.
//                      \-> 0.
fn test_gas_for_fee() {
    // First branch - 3 nested calls.
    let mut inner_calls = vec![];
    for (tracked_resource, gas_consumed, expected_gas_for_fee) in [
        (TrackedResource::CairoSteps, 1, 0),
        (TrackedResource::CairoSteps, 2, 0),
        (TrackedResource::SierraGas, 3, 1),
    ] {
        assert_eq!(
            to_gas_for_fee(&tracked_resource, gas_consumed, &inner_calls).0,
            expected_gas_for_fee
        );
        inner_calls = vec![CallInfo {
            execution: CallExecution { gas_consumed, ..Default::default() },
            tracked_resource,
            inner_calls,
            charged_resources: ChargedResources {
                gas_for_fee: GasAmount(expected_gas_for_fee),
                ..Default::default()
            },
            ..Default::default()
        }];
    }

    // Second branch - 1 call.
    let (tracked_resource, gas_consumed, expected_gas_for_fee) =
        (TrackedResource::CairoSteps, 4, 0);
    assert_eq!(to_gas_for_fee(&tracked_resource, gas_consumed, &[]).0, expected_gas_for_fee);

    inner_calls.push(CallInfo {
        execution: CallExecution { gas_consumed, ..Default::default() },
        tracked_resource,
        charged_resources: ChargedResources {
            gas_for_fee: GasAmount(expected_gas_for_fee),
            ..Default::default()
        },
        ..Default::default()
    });

    // Outer call.
    let (tracked_resource, gas_consumed, expected_gas_for_fee) = (TrackedResource::SierraGas, 8, 2);
    assert_eq!(
        to_gas_for_fee(&tracked_resource, gas_consumed, &inner_calls).0,
        expected_gas_for_fee
    );
}
