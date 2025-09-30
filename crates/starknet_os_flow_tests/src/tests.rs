use std::collections::HashMap;
use std::sync::LazyLock;

use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::abi::abi_utils::{get_storage_var_address, selector_from_name};
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::core::{calculate_contract_address, Nonce};
use starknet_api::executable_transaction::{
    DeclareTransaction,
    L1HandlerTransaction as ExecutableL1HandlerTransaction,
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::{
    CHAIN_ID_FOR_TESTS,
    DEFAULT_STRK_L1_DATA_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    DEFAULT_STRK_L2_GAS_PRICE,
};
use starknet_api::transaction::constants::DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    Fee,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_api::transaction::L1HandlerTransaction;
use starknet_api::{calldata, declare_tx_args, invoke_tx_args};
use starknet_committer::block_committer::input::{
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_types_core::felt::Felt;

use crate::initial_state::create_default_initial_state_data;
use crate::test_manager::{TestManager, TestParameters, FUNDED_ACCOUNT_ADDRESS};
use crate::utils::{divide_vec_into_n_parts, get_class_info_of_feature_contract};

pub(crate) static NON_TRIVIAL_RESOURCE_BOUNDS: LazyLock<ValidResourceBounds> =
    LazyLock::new(|| {
        ValidResourceBounds::AllResources(AllResourceBounds {
            l1_gas: ResourceBounds {
                max_amount: GasAmount(100_000_000),
                max_price_per_unit: DEFAULT_STRK_L1_GAS_PRICE.into(),
            },
            l2_gas: ResourceBounds {
                max_amount: GasAmount(100_000_000_000_000_000),
                max_price_per_unit: DEFAULT_STRK_L2_GAS_PRICE.into(),
            },
            l1_data_gas: ResourceBounds {
                max_amount: GasAmount(100_000),
                max_price_per_unit: DEFAULT_STRK_L1_DATA_GAS_PRICE.into(),
            },
        })
    });

#[tokio::test]
async fn test_initial_state_creation() {
    let _initial_state = create_default_initial_state_data::<DictStateReader, 0>([]).await;
}

#[rstest]
#[case(10, 2, vec![5, 5])]
#[case(10, 3, vec![4, 3, 3])]
#[case(10, 4, vec![3, 3, 2, 2])]
#[case(8, 5, vec![2, 2, 2, 1, 1])]
#[case(0, 1, vec![0])]
#[case(0, 2, vec![0, 0])]
#[case(1, 1, vec![1])]
#[case(12, 7, vec![2, 2, 2, 2, 2, 1, 1])]
fn division(#[case] length: usize, #[case] n_parts: usize, #[case] expected_lengths: Vec<usize>) {
    let to_divide = vec![0; length];
    let divided = divide_vec_into_n_parts(to_divide, n_parts);
    let actual_lengths: Vec<usize> = divided.iter().map(|part| part.len()).collect();
    assert_eq!(actual_lengths, expected_lengths);
}

/// Scenario of declaring and deploying the test contract.
#[rstest]
#[tokio::test]
async fn declare_deploy_scenario(
    #[values(1, 2)] n_blocks: usize,
    #[values(false, true)] use_kzg_da: bool,
    #[values(false, true)] full_output: bool,
) {
    // Initialize the test manager with a default initial state and get the nonce manager to help
    // keep track of nonces.

    let (mut test_manager, _) =
        TestManager::<DictStateReader>::new_with_default_initial_state([]).await;

    // Declare a test contract.
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let test_contract_sierra = test_contract.get_sierra();
    let class_hash = test_contract_sierra.calculate_class_hash();
    let compiled_class_hash = test_contract.get_compiled_class_hash(&HashVersion::V2);
    let declare_tx_args = declare_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        class_hash,
        compiled_class_hash,
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        nonce: test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
    };
    let account_declare_tx = declare_tx(declare_tx_args);
    let class_info = get_class_info_of_feature_contract(test_contract);
    let tx =
        DeclareTransaction::create(account_declare_tx, class_info, &CHAIN_ID_FOR_TESTS).unwrap();
    // Add the transaction to the test manager.
    test_manager.add_cairo1_declare_tx(tx, &test_contract_sierra);
    let arg1 = Felt::from(7);
    let arg2 = Felt::from(90);
    // Deploy the test contract using the deploy contract syscall.
    let constructor_calldata = [
        2.into(), // constructor length
        arg1,
        arg2,
    ];
    let contract_address_salt = ContractAddressSalt(Felt::ONE);
    let calldata: Vec<_> =
        [class_hash.0, contract_address_salt.0].into_iter().chain(constructor_calldata).collect();
    let deploy_contract_calldata = create_calldata(
        *FUNDED_ACCOUNT_ADDRESS,
        DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME,
        &calldata,
    );
    let expected_contract_address = calculate_contract_address(
        contract_address_salt,
        class_hash,
        &Calldata(constructor_calldata[1..].to_vec().into()),
        *FUNDED_ACCOUNT_ADDRESS,
    )
    .unwrap();
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata: deploy_contract_calldata });
    test_manager.divide_transactions_into_n_blocks(n_blocks);
    let test_output = test_manager
        .execute_test_with_default_block_contexts(&TestParameters {
            use_kzg_da,
            full_output,
            ..Default::default()
        })
        .await;

    let partial_state_diff = StateDiff {
        // Deployed contract.
        address_to_class_hash: HashMap::from([(expected_contract_address, class_hash)]),
        // Storage update from the contract's constructor.
        storage_updates: HashMap::from([(
            expected_contract_address,
            HashMap::from([(
                StarknetStorageKey(get_storage_var_address("my_storage_var", &[])),
                StarknetStorageValue(arg1 + arg2),
            )]),
        )]),
        // Declared class.
        class_hash_to_compiled_class_hash: HashMap::from([(
            class_hash,
            CompiledClassHash(compiled_class_hash.0),
        )]),
        ..Default::default()
    };

    let perform_global_validations = true;
    test_output.perform_validations(perform_global_validations, Some(&partial_state_diff));
}

/// Test state diffs in separate blocks that become trivial in a multiblock.
#[rstest]
#[tokio::test]
async fn trivial_diff_scenario(
    #[values(false, true)] use_kzg_da: bool,
    #[values(false, true)] full_output: bool,
    #[values(
        FeatureContract::TestContract(CairoVersion::Cairo0),
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm))
    )]
    test_contract: FeatureContract,
) {
    // Initialize the test manager with a default initial state and get the nonce manager to help
    // keep track of nonces.

    let (mut test_manager, [test_contract_address]) =
        TestManager::<DictStateReader>::new_with_default_initial_state([(
            test_contract,
            calldata![Felt::ONE, Felt::TWO],
        )])
        .await;

    let key = Felt::from(10u8);
    let value = Felt::from(11u8);
    let function_name = "test_storage_read_write";
    // Invoke a function on the test contract that changes the key to the new value.
    let calldata = create_calldata(test_contract_address, function_name, &[key, value]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Move to next block, and add an invoke that reverts the previous change.
    test_manager.move_to_next_block();
    let calldata = create_calldata(test_contract_address, function_name, &[key, Felt::ZERO]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Execute the test.
    let test_output = test_manager
        .execute_test_with_default_block_contexts(&TestParameters {
            use_kzg_da,
            full_output,
            ..Default::default()
        })
        .await;

    // Explicitly check the test contract has no storage update.
    assert!(
        !test_output.decompressed_state_diff.storage_updates.contains_key(&test_contract_address)
    );

    test_output.perform_default_validations();
}

/// This test verifies that when an entry point modifies storage and then reverts (panics):
/// 1. All storage changes made before the revert are properly rolled back.
/// 2. The transaction fee is still deducted from the caller's account.
#[rstest]
#[case::cairo0(
    FeatureContract::TestContract(CairoVersion::Cairo0),
    "ASSERT_EQ instruction failed: 1 != 0"
)]
#[case::cairo1(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    "Panic for revert"
)]
#[tokio::test]
async fn test_reverted_invoke_tx(
    #[case] test_contract: FeatureContract,
    #[case] revert_reason: &str,
) {
    let (use_kzg_da, full_output) = (true, false);

    let (mut test_manager, [test_contract_address]) =
        TestManager::<DictStateReader>::new_with_default_initial_state([(
            test_contract,
            calldata![Felt::ONE, Felt::TWO],
        )])
        .await;

    // Call a reverting function that changes the storage.
    let invoke_tx_args = invoke_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        nonce: test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        calldata: create_calldata(test_contract_address, "write_and_revert", &[Felt::ONE, Felt::TWO]),
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
    };
    test_manager.add_invoke_tx_from_args(
        invoke_tx_args,
        &CHAIN_ID_FOR_TESTS,
        Some(revert_reason.to_string()),
    );

    // Execute the test.
    let test_output = test_manager
        .execute_test_with_default_block_contexts(&TestParameters {
            use_kzg_da,
            full_output,
            ..Default::default()
        })
        .await;

    // Check that the storage was reverted (no change in test contract address).
    assert!(
        !test_output.decompressed_state_diff.storage_updates.contains_key(&test_contract_address)
    );
    // Check that a fee was deducted.
    test_output.assert_account_balance_change(*FUNDED_ACCOUNT_ADDRESS);

    test_output.perform_default_validations();
}

/// Verifies that when an L1 handler modifies storage and then reverts, all storage changes made
/// before the revert are properly rolled back.
#[rstest]
#[case::cairo0(
    FeatureContract::TestContract(CairoVersion::Cairo0),
    "ASSERT_EQ instruction failed: 1 != 0."
)]
#[case::cairo1(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    "revert in l1 handler"
)]
#[tokio::test]
async fn test_reverted_l1_handler_tx(
    #[case] test_contract: FeatureContract,
    #[case] revert_reason: &str,
) {
    let (mut test_manager, [test_contract_address]) =
        TestManager::<DictStateReader>::new_with_default_initial_state([(
            test_contract,
            calldata![Felt::ONE, Felt::TWO],
        )])
        .await;

    // Add a reverting L1 handler transaction that changes the storage.
    let tx = ExecutableL1HandlerTransaction::create(
        L1HandlerTransaction {
            version: L1HandlerTransaction::VERSION,
            nonce: Nonce::default(),
            contract_address: test_contract_address,
            entry_point_selector: selector_from_name("l1_handler_set_value_and_revert"),
            // from_address (L1 address), key, value.
            calldata: calldata![Felt::THREE, Felt::ONE, Felt::TWO],
        },
        &CHAIN_ID_FOR_TESTS,
        Fee(1_000_000),
    )
    .unwrap();
    test_manager.add_l1_handler_tx(tx, Some(revert_reason.to_string()));

    let test_output =
        test_manager.execute_test_with_default_block_contexts(&TestParameters::default()).await;

    // Check that the storage was reverted (no change in test contract address).
    assert!(
        !test_output.decompressed_state_diff.storage_updates.contains_key(&test_contract_address)
    );
    // Make sure we expect no messages were sent to L2, explicitly, before validating actual output.
    assert!(test_output.expected_values.messages_to_l2.is_empty());
    test_output.perform_default_validations();
}
