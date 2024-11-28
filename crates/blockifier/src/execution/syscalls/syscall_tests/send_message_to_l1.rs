use itertools::concat;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::EthAddress;
use starknet_api::felt;
use starknet_api::transaction::fields::Calldata;
use starknet_api::transaction::L2ToL1Payload;
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::call_info::{CallExecution, MessageToL1, OrderedL2ToL1Message};
use crate::execution::entry_point::CallEntryPoint;
use crate::test_utils::contracts::{
    FeatureContract,
    RunnableContractVersion,
};
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, BALANCE};

#[cfg_attr(
    feature = "cairo_native",
    test_case(RunnableContractVersion::Cairo1(RunnableCairo1ContractVersion::Native); "Native")
)]
#[test_case(RunnableContractVersion::Cairo1Casm;"VM")]
fn test_send_message_to_l1(cairo_version: RunnableContractVersion) {
    let test_contract = FeatureContract::TestContract(cairo_version);
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let to_address = felt!(1234_u16);
    let payload = vec![felt!(2019_u16), felt!(2020_u16), felt!(2021_u16)];
    let calldata = Calldata(
        concat(vec![
            vec![
                to_address,
                // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the
                // convertion works.
                felt!(u64::try_from(payload.len()).expect("Failed to convert usize to u64.")),
            ],
            payload.clone(),
        ])
        .into(),
    );
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_send_message_to_l1"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let to_address = EthAddress::try_from(to_address).unwrap();
    let message = MessageToL1 { to_address, payload: L2ToL1Payload(payload) };

    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution {
            l2_to_l1_messages: vec![OrderedL2ToL1Message { order: 0, message }],
            gas_consumed: 20960,
            ..Default::default()
        }
    );
}
