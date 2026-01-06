use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::core::EthAddress;
use starknet_api::transaction::{L2ToL1Payload, MessageToL1};
use starknet_api::{calldata, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::test_manager::TestBuilder;

#[rstest]
#[tokio::test]
async fn test_basic_happy_flow() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    let (mut test_builder, [contract_address]) =
        TestBuilder::create_standard_virtual([(test_contract, calldata![Felt::ONE, Felt::TWO])])
            .await;

    let to_address = Felt::from(85);
    let payload = vec![Felt::from(12), Felt::from(34)];
    let calldata = create_calldata(
        contract_address,
        "test_send_message_to_l1",
        &[to_address, Felt::from(payload.len()), payload[0], payload[1]],
    );
    test_builder.add_funded_account_invoke(invoke_tx_args! { calldata });
    test_builder.messages_to_l1.push(MessageToL1 {
        from_address: contract_address,
        to_address: EthAddress::try_from(to_address).unwrap(),
        payload: L2ToL1Payload(payload),
    });

    test_builder.build().await.run_virtual_and_validate();
}
