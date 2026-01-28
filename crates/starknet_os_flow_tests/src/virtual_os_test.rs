use blockifier::context::BlockContext;
use blockifier::execution::contract_class::TrackedResource;
use blockifier::test_utils::block_hash_contract_address;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::transaction::test_utils::ExpectedExecutionInfo;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::BlockTimestamp;
use starknet_api::core::EthAddress;
use starknet_api::state::StorageKey;
use starknet_os::io::virtual_os_output::VirtualOsOutput;

/// STORED_BLOCK_HASH_BUFFER from blockifier constants.
const STORED_BLOCK_HASH_BUFFER: u64 = 10;
use starknet_api::test_utils::{
    CURRENT_BLOCK_TIMESTAMP,
    TEST_SEQUENCER_ADDRESS,
    VIRTUAL_OS_PROGRAM_HASH,
};
use starknet_api::transaction::fields::{ProofFacts, TransactionSignature, VIRTUAL_SNOS};
use starknet_api::transaction::{
    InvokeTransaction as ApiInvokeTransaction,
    L2ToL1Payload,
    MessageToL1,
    TransactionVersion,
};
use starknet_api::{calldata, contract_address, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::initial_state::{create_default_initial_state_data, InitialStateData};
use crate::test_manager::{TestBuilder, TestBuilderConfig, FUNDED_ACCOUNT_ADDRESS};
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
    test_builder.add_funded_account_invoke(invoke_tx_args! { calldata });

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
        ProofFacts::snos_proof_facts_for_testing_with_config_hash(
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

    test_builder.add_invoke_tx(tx, None);

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
    test_builder.add_invoke_tx(tx, Some("Panic for revert".to_string()));

    test_builder
        .build()
        .await
        .run_virtual_expect_error("Reverted transactions are not supported in virtual OS mode");
}

#[tokio::test]
/// Tests the full flow: run virtual OS, use its output as proof_facts, then run in non-virtual
/// mode.
async fn test_virtual_os_output_as_proof_facts() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    // Create initial state data once and clone for both runners.
    let (initial_state_data, [contract_address]): (InitialStateData<DictStateReader>, _) =
        create_default_initial_state_data([(test_contract, calldata![Felt::ONE, Felt::TWO])]).await;
    let mut initial_state_data_for_non_virtual = initial_state_data.clone();

    // Run the virtual OS.
    let mut virtual_test_builder = TestBuilder::new_with_initial_state_data(
        initial_state_data,
        TestBuilderConfig::default(),
        true, // virtual mode
    );

    // TODO(Yoni): replace read with message to L1.
    let calldata = create_calldata(contract_address, "test_storage_read", &[Felt::ONE]);
    virtual_test_builder.add_funded_account_invoke(invoke_tx_args! { calldata: calldata.clone() });

    let virtual_os_output = virtual_test_builder.build().await.run_virtual();
    virtual_os_output.validate();

    // Parse the raw output to extract block number and hash.
    let raw_output = &virtual_os_output.runner_output.raw_output;
    let parsed_output = VirtualOsOutput::from_raw_output(raw_output).unwrap();

    // TODO(Yoni): run the prover and get the proof_facts from it.

    // Convert to proof_facts by prepending VIRTUAL_SNOS and the program hash.
    let mut proof_facts_data = vec![Felt::from(VIRTUAL_SNOS), VIRTUAL_OS_PROGRAM_HASH];
    proof_facts_data.extend(raw_output.iter().copied());
    let proof_facts = ProofFacts::from(proof_facts_data);
    let proof_block_number = parsed_output.base_block_number;

    // Add the proof's block hash to the block hash contract in the cloned state.
    initial_state_data_for_non_virtual.initial_state.updatable_state.storage_view.insert(
        (
            block_hash_contract_address(),
            StorageKey::try_from(Felt::from(proof_block_number.0)).unwrap(),
        ),
        parsed_output.base_block_hash,
    );

    // Update block context to be at a later block number so the proof is not "too recent".
    // The proof's block number must be < current_block_number - STORED_BLOCK_HASH_BUFFER.
    let base_block_context = initial_state_data_for_non_virtual.initial_state.block_context.clone();
    initial_state_data_for_non_virtual.initial_state.block_context =
        BlockContext::from_base_context(
            &base_block_context,
            STORED_BLOCK_HASH_BUFFER.try_into().unwrap(),
            false,
        );

    let mut non_virtual_test_builder = TestBuilder::new_with_initial_state_data(
        initial_state_data_for_non_virtual,
        TestBuilderConfig::default(),
        false, // non-virtual mode
    );

    let calldata = create_calldata(contract_address, "test_storage_read", &[Felt::ONE]);
    non_virtual_test_builder.add_funded_account_invoke(invoke_tx_args! { calldata, proof_facts });

    // Run in non-virtual mode and validate.
    non_virtual_test_builder.build().await.run().perform_default_validations();
}
