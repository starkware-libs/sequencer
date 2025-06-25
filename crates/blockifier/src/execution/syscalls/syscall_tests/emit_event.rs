use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect;
use itertools::concat;
#[cfg(feature = "cairo_native")]
use pretty_assertions::assert_eq;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::felt;
use starknet_api::transaction::fields::Calldata;
use starknet_api::transaction::{EventContent, EventData, EventKey};
use starknet_types_core::felt::Felt;
use test_case::test_case;

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::context::ChainInfo;
use crate::execution::call_info::{CallInfo, OrderedEvent};
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::errors::EntryPointExecutionError;
use crate::execution::syscalls::hint_processor::EmitEventError;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, BALANCE};

const KEYS: [Felt; 2] = [Felt::from_hex_unchecked("0x2019"), Felt::from_hex_unchecked("0x2020")];
const DATA: [Felt; 3] = [
    Felt::from_hex_unchecked("0x2021"),
    Felt::from_hex_unchecked("0x2022"),
    Felt::from_hex_unchecked("0x2023"),
];
const N_EMITTED_EVENTS: [Felt; 1] = [Felt::from_hex_unchecked("0x1")];

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
#[test_case(RunnableCairo1::Casm;"VM")]
fn positive_flow(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let call_info = emit_events(test_contract, &N_EMITTED_EVENTS, &KEYS, &DATA)
        .expect("emit_events failed with valued parameters");
    let event = EventContent {
        keys: KEYS.into_iter().map(EventKey).collect(),
        data: EventData(DATA.to_vec()),
    };

    if runnable_version.is_cairo_native() {
        expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [
                OrderedEvent {
                    order: 0,
                    event: EventContent {
                        keys: [
                            EventKey(
                                0x2019,
                            ),
                            EventKey(
                                0x2020,
                            ),
                        ],
                        data: EventData(
                            [
                                0x2021,
                                0x2022,
                                0x2023,
                            ],
                        ),
                    },
                },
            ],
            l2_to_l1_messages: [],
            cairo_native: true,
            failed: false,
            gas_consumed: 34580,
        }
    "#]]
    } else {
        expect![[r#"
            CallExecution {
                retdata: Retdata(
                    [],
                ),
                events: [
                    OrderedEvent {
                        order: 0,
                        event: EventContent {
                            keys: [
                                EventKey(
                                    0x2019,
                                ),
                                EventKey(
                                    0x2020,
                                ),
                            ],
                            data: EventData(
                                [
                                    0x2021,
                                    0x2022,
                                    0x2023,
                                ],
                            ),
                        },
                    },
                ],
                l2_to_l1_messages: [],
                cairo_native: false,
                failed: false,
                gas_consumed: 34580,
            }
        "#]]
    }
    .assert_debug_eq(&call_info.execution);
    assert_eq!(call_info.execution.events, vec![OrderedEvent { order: 0, event }]);
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
#[test_case(RunnableCairo1::Casm;"VM")]
fn data_length_exceeds_limit(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let versioned_constants = VersionedConstants::create_for_testing();

    let max_event_data_length = versioned_constants.tx_event_limits.max_data_length;
    let data_too_long = vec![felt!(2_u16); max_event_data_length + 1];

    let call_result = emit_events(test_contract, &N_EMITTED_EVENTS, &KEYS, &data_too_long);

    let error_message = call_result.unwrap_err().to_string();

    let expected_error = EmitEventError::ExceedsMaxDataLength {
        data_length: max_event_data_length + 1,
        max_data_length: max_event_data_length,
    };
    assert!(error_message.contains(&expected_error.to_string()));
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
#[test_case(RunnableCairo1::Casm;"VM")]
fn keys_length_exceeds_limit(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let versioned_constants = VersionedConstants::create_for_testing();

    let max_event_keys_length = versioned_constants.tx_event_limits.max_keys_length;
    let keys_too_long = vec![felt!(1_u16); max_event_keys_length + 1];

    let call_result = emit_events(test_contract, &N_EMITTED_EVENTS, &keys_too_long, &DATA);

    let error_message = call_result.unwrap_err().to_string();

    let expected_error = EmitEventError::ExceedsMaxKeysLength {
        keys_length: max_event_keys_length + 1,
        max_keys_length: max_event_keys_length,
    };

    assert!(error_message.contains(&expected_error.to_string()));
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
#[test_case(RunnableCairo1::Casm;"VM")]
fn event_number_exceeds_limit(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let versioned_constants = VersionedConstants::create_for_testing();

    let max_n_emitted_events = versioned_constants.tx_event_limits.max_n_emitted_events;
    let n_emitted_events_too_big = vec![felt!(
        u16::try_from(max_n_emitted_events + 1).expect("Failed to convert usize to u16.")
    )];
    let call_result = emit_events(test_contract, &n_emitted_events_too_big, &KEYS, &DATA);

    let error_message = call_result.unwrap_err().to_string();

    let expected_error = EmitEventError::ExceedsMaxNumberOfEmittedEvents {
        n_emitted_events: max_n_emitted_events + 1,
        max_n_emitted_events,
    };
    assert!(error_message.contains(&expected_error.to_string()));
}

#[allow(clippy::result_large_err)]
fn emit_events(
    test_contract: FeatureContract,
    n_emitted_events: &[Felt],
    keys: &[Felt],
    data: &[Felt],
) -> Result<CallInfo, EntryPointExecutionError> {
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);
    let calldata = Calldata(
        concat(vec![
            n_emitted_events.to_owned(),
            vec![felt!(u16::try_from(keys.len()).expect("Failed to convert usize to u16."))],
            keys.to_vec(),
            vec![felt!(u16::try_from(data.len()).expect("Failed to convert usize to u16."))],
            data.to_vec(),
        ])
        .into(),
    );

    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_emit_events"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    entry_point_call.execute_directly(&mut state)
}
