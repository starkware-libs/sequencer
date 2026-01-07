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

/// Security tests.
/// Note that it's important to construct the hints correctly and get the error directly from Cairo
/// (and not from the blockifier), as users can submit virtual OS proofs with arbitrary hints.

#[rstest]
#[tokio::test]
/// Test that the virtual OS fails when more than one transaction is added.
async fn test_two_txs_os_error() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    let (mut test_builder, [contract_address]) =
        TestBuilder::create_standard_virtual([(test_contract, calldata![Felt::ONE, Felt::TWO])])
            .await;

    // Add first invoke transaction.
    let calldata = create_calldata(contract_address, "test_storage_read", &[Felt::ONE]);
    test_builder.add_funded_account_invoke(invoke_tx_args! { calldata: calldata.clone() });

    // Add second invoke transaction - this should cause the virtual OS to fail.
    test_builder.add_funded_account_invoke(invoke_tx_args! { calldata });

    test_builder.build().await.run_virtual_expect_error("Expected exactly one transaction");
}

#[rstest]
#[tokio::test]
/// Test that the virtual OS fails when a non-invoke transaction is added.
async fn test_non_invoke_tx_os_error() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    let (mut test_builder, [contract_address]) =
        TestBuilder::create_standard_virtual([(test_contract, calldata![Felt::ONE, Felt::TWO])])
            .await;

    // Add an L1 handler transaction instead of an invoke.
    // TODO(Yoni): parameterize other transaction types.
    test_builder.add_l1_handler(
        contract_address,
        "l1_handle",
        calldata![Felt::ONE, Felt::TWO],
        None,
    );

    test_builder.build().await.run_virtual_expect_error("Expected INVOKE_FUNCTION transaction");
}

#[rstest]
#[tokio::test]
/// Test that the virtual OS fails when invoking a Cairo 0 contract.
async fn test_cairo0_contract_os_error() {
    let (mut test_builder, [contract_address]) = TestBuilder::create_standard_virtual([(
        FeatureContract::TestContract(CairoVersion::Cairo0),
        calldata![Felt::ZERO, Felt::ZERO],
    )])
    .await;

    let calldata = create_calldata(contract_address, "foo", &[]);
    test_builder.add_funded_account_invoke(invoke_tx_args! { calldata });

    // The OS tries to run it as a Cairo 1 contract and cannot find the compiled class.
    // (the key 0 is the "compiled class hash" of the Cairo 0 contract).
    test_builder
        .build()
        .await
        .run_virtual_expect_error("find_element(): No value found for key: 0");
}

// TODO(Yoni): add a test for a Cairo 1 contract that is not a Sierra 1.7.0+ contract.
