use blockifier::execution::contract_class::TrackedResource;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::get_valid_virtual_os_program_hash;
use blockifier::transaction::test_utils::ExpectedExecutionInfo;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::BlockTimestamp;
use starknet_api::core::EthAddress;
use starknet_api::test_utils::{
    test_block_hash,
    BLOCK_HASH_HISTORY_RANGE,
    CURRENT_BLOCK_NUMBER,
    CURRENT_BLOCK_TIMESTAMP,
    TEST_SEQUENCER_ADDRESS,
};
use starknet_api::transaction::fields::{
    ProofFacts,
    TransactionSignature,
    PROOF_VERSION,
    VIRTUAL_OS_OUTPUT_VERSION,
    VIRTUAL_SNOS,
};
use starknet_api::transaction::{
    InvokeTransaction as ApiInvokeTransaction,
    L2ToL1Payload,
    MessageToL1,
    TransactionVersion,
};
use starknet_api::{calldata, contract_address, felt, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::test_manager::{
    EventPredicateExpectation,
    TestBuilder,
    TestBuilderConfig,
    FUNDED_ACCOUNT_ADDRESS,
};
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;

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

    let mut test_runner = test_builder.build().await;
    // Patch the tracked resources of the Cairo 0 call to bypass this validation and get the
    // expected Cairo 0 error from the OS.
    test_runner
        .os_hints
        .os_input
        .os_block_inputs
        .first_mut()
        .unwrap()
        .tx_execution_infos
        .first_mut()
        .unwrap()
        .execute_call_info
        .as_mut()
        .unwrap()
        .inner_calls
        .first_mut()
        .unwrap()
        .tracked_resource = TrackedResource::SierraGas;

    // The OS tries to run it as a Cairo 1 contract and cannot find the compiled class.
    // (the key 0 is the "compiled class hash" of the Cairo 0 contract).
    test_runner.run_virtual_expect_error("find_element(): No value found for key: 0");
}

// TODO(Yoni): add a test for a Cairo 1 contract that is not a Sierra 1.7.0+ contract.

#[rstest]
#[case::deploy("Deploy")]
#[case::get_block_hash("GetBlockHash")]
#[case::keccak("Keccak")]
#[case::replace_class("ReplaceClass")]
#[case::meta_tx_v0("MetaTxV0")]
#[tokio::test]
async fn test_forbidden_syscall(#[case] selector: &str) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let (mut test_builder, [contract_address]) =
        TestBuilder::create_standard_virtual([(test_contract, calldata![Felt::ONE, Felt::TWO])])
            .await;

    let selector_felt = Felt::from_bytes_be_slice(selector.as_bytes());
    let calldata = create_calldata(
        contract_address,
        "test_forbidden_syscall_in_virtual_mode",
        &[selector_felt],
    );
    let mut event_expectations = Vec::new();
    if selector == "MetaTxV0" {
        event_expectations.push(EventPredicateExpectation {
            description: "MetaTxV0 emits a contract event".to_string(),
            predicate: Box::new(move |event| event.from_address == contract_address),
        });
    }
    test_builder
        .add_funded_account_invoke_with_events(invoke_tx_args! { calldata }, event_expectations);

    let expected_error = format!("Unexpected syscall selector in virtual mode: {selector_felt}.");
    test_builder.build().await.run_virtual_expect_error(&expected_error);
}

// TODO(Yoni): consider adding a positive test for all supported syscalls.

#[rstest]
#[case::virtual_mode(true)]
#[case::regular_mode(false)]
#[tokio::test]
/// Tests that get_execution_info returns the correct block info.
/// In virtual OS mode, the base (previous) block info is returned.
/// In regular mode, the current block info is returned.
async fn test_get_execution_info(#[case] virtual_os: bool) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let (mut test_builder, [contract_address]) =
        TestBuilder::<DictStateReader>::new_with_default_initial_state(
            [(test_contract, calldata![Felt::ONE, Felt::TWO])],
            TestBuilderConfig::default(),
            virtual_os,
        )
        .await;

    // In virtual OS mode, get_execution_info returns the base block info (the previous block).
    // In regular mode, it returns the current block info.
    let base_block_info = test_builder.base_block_info();
    let (block_number, block_timestamp, sequencer_address) = if virtual_os {
        (
            base_block_info.block_number,
            base_block_info.block_timestamp,
            base_block_info.sequencer_address,
        )
    } else {
        (
            base_block_info.block_number.next().unwrap(),
            BlockTimestamp(CURRENT_BLOCK_TIMESTAMP),
            contract_address!(TEST_SEQUENCER_ADDRESS),
        )
    };

    let selector = selector_from_name("test_get_execution_info_v3");
    let proof_facts = if virtual_os {
        // Non-empty proof facts are not supported in virtual OS mode.
        ProofFacts::default()
    } else {
        ProofFacts::custom_proof_facts_for_testing(
            get_valid_virtual_os_program_hash(),
            test_builder.compute_virtual_os_config_hash(),
        )
    };
    let expected_execution_info = ExpectedExecutionInfo {
        version: TransactionVersion::THREE,
        account_address: *FUNDED_ACCOUNT_ADDRESS,
        caller_address: *FUNDED_ACCOUNT_ADDRESS,
        contract_address,
        chain_id: Some(test_builder.chain_id()),
        entry_point_selector: selector,
        block_number,
        block_timestamp,
        sequencer_address,
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        nonce: test_builder.get_nonce(*FUNDED_ACCOUNT_ADDRESS),
        proof_facts: proof_facts.clone(),
        ..Default::default()
    }
    .to_syscall_result();

    let calldata =
        create_calldata(contract_address, "test_get_execution_info_v3", &expected_execution_info);
    let mut tx =
        test_builder.create_funded_account_invoke(invoke_tx_args! { calldata, proof_facts });
    let ApiInvokeTransaction::V3(tx_v3) = &mut tx.tx else { unreachable!() };
    tx_v3.signature = TransactionSignature(vec![tx.tx_hash.0].into());

    test_builder.add_invoke_tx(tx, None, None);

    if virtual_os {
        test_builder.build().await.run_virtual_and_validate();
    } else {
        test_builder.build().await.run().perform_default_validations();
    }
}

#[tokio::test]
/// Test that the virtual OS fails when a reverted transaction is added.
async fn test_reverted_tx_os_error() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    let (mut test_builder, [contract_address]) =
        TestBuilder::create_standard_virtual([(test_contract, calldata![Felt::ONE, Felt::TWO])])
            .await;

    // Add a reverting invoke transaction.
    let calldata = create_calldata(contract_address, "write_and_revert", &[Felt::ONE, Felt::TWO]);
    let tx = test_builder.create_funded_account_invoke(invoke_tx_args! { calldata });
    test_builder.add_invoke_tx(tx, Some("Panic for revert".to_string()), None);

    test_builder
        .build()
        .await
        .run_virtual_expect_error("Reverted transactions are not supported in virtual OS mode");
}

/// End-to-end test: virtual OS produces a Blake2s message hash, then a regular OS transaction
/// independently computes the same hash in Cairo1 and verifies it matches the proof facts.
#[tokio::test]
async fn test_blake_message_hash_virtual_to_regular_os() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    // --- Step 1: Run the virtual OS with a transaction that sends an L2-to-L1 message ---
    let (mut virtual_builder, [virtual_contract_address]) =
        TestBuilder::create_standard_virtual([(test_contract, calldata![Felt::ONE, Felt::TWO])])
            .await;

    let to_address = Felt::from(85);
    let payload = vec![Felt::from(12), Felt::from(34)];
    let calldata = create_calldata(
        virtual_contract_address,
        "test_send_message_to_l1",
        &[to_address, Felt::from(payload.len()), payload[0], payload[1]],
    );
    virtual_builder.add_funded_account_invoke(invoke_tx_args! { calldata });
    virtual_builder.messages_to_l1.push(MessageToL1 {
        from_address: virtual_contract_address,
        to_address: EthAddress::try_from(to_address).unwrap(),
        payload: L2ToL1Payload(payload.clone()),
    });

    let virtual_output = virtual_builder.build().await.run_virtual();
    virtual_output.validate();

    // Extract the Blake message hash produced by the virtual OS.
    let virtual_os_output = starknet_os::io::virtual_os_output::VirtualOsOutput::from_raw_output(
        &virtual_output.runner_output.raw_output,
    )
    .expect("Parsing virtual OS output should not fail.");
    assert_eq!(virtual_os_output.messages_to_l1_hashes.len(), 1);
    let blake_message_hash = virtual_os_output.messages_to_l1_hashes[0];

    // --- Step 2: Construct proof facts with valid test block numbers and the real hash ---
    let program_hash = get_valid_virtual_os_program_hash();
    let (mut regular_builder, [regular_contract_address]) =
        TestBuilder::create_standard([(test_contract, calldata![Felt::ONE, Felt::TWO])]).await;
    let config_hash = regular_builder.compute_virtual_os_config_hash();

    let block_hash_history_start = CURRENT_BLOCK_NUMBER - BLOCK_HASH_HISTORY_RANGE;
    let block_number_u64 = block_hash_history_start + 2;
    let block_number = felt!(block_number_u64);
    let block_hash = test_block_hash(block_number_u64).0;
    let n_messages = Felt::ONE;

    let proof_facts = ProofFacts(
        vec![
            PROOF_VERSION,
            VIRTUAL_SNOS,
            program_hash,
            VIRTUAL_OS_OUTPUT_VERSION,
            block_number,
            block_hash,
            config_hash,
            n_messages,
            blake_message_hash,
        ]
        .into(),
    );

    // --- Step 3: Regular OS run that verifies the hash in Cairo1 ---
    // The message data as the contract would see it: [from_address, to_address, payload_size,
    // ...payload].
    let message_data: Vec<Felt> = vec![
        *virtual_contract_address.0.key(),
        to_address,
        Felt::from(payload.len()),
        payload[0],
        payload[1],
    ];
    // ABI-encode the Span<felt252> parameter: [length, elements...].
    let mut entry_point_args: Vec<Felt> = vec![Felt::from(message_data.len())];
    entry_point_args.extend_from_slice(&message_data);
    let calldata = create_calldata(
        regular_contract_address,
        "test_verify_virtual_os_message_hash",
        &entry_point_args,
    );

    regular_builder.add_funded_account_invoke(invoke_tx_args! { calldata, proof_facts });

    regular_builder.build().await.run().perform_default_validations();
}
