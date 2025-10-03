use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::abi::abi_utils::{get_storage_var_address, selector_from_name};
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    ContractAddress,
    EthAddress,
    Nonce,
};
use starknet_api::executable_transaction::{
    DeclareTransaction,
    InvokeTransaction,
    L1HandlerTransaction as ExecutableL1HandlerTransaction,
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::StorageKey;
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::test_utils::{
    CHAIN_ID_FOR_TESTS,
    CURRENT_BLOCK_TIMESTAMP,
    DEFAULT_STRK_L1_DATA_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    DEFAULT_STRK_L2_GAS_PRICE,
    TEST_SEQUENCER_ADDRESS,
};
use starknet_api::transaction::constants::DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    Fee,
    ResourceBounds,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::{
    L1HandlerTransaction,
    L1ToL2Payload,
    L2ToL1Payload,
    MessageToL1,
    TransactionVersion,
};
use starknet_api::{calldata, declare_tx_args, invoke_tx_args};
use starknet_committer::block_committer::input::{
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_core::crypto::ecdsa_sign;
use starknet_crypto::{get_public_key, Signature};
use starknet_os::hints::hint_implementation::deprecated_compiled_class::class_hash::compute_deprecated_class_hash;
use starknet_os::io::os_output::MessageToL2;
use starknet_types_core::felt::Felt;

use crate::initial_state::{
    create_default_initial_state_data,
    get_deploy_contract_tx_and_address_with_salt,
};
use crate::special_contracts::{
    V1_BOUND_CAIRO0_CONTRACT,
    V1_BOUND_CAIRO1_CONTRACT_CASM,
    V1_BOUND_CAIRO1_CONTRACT_SIERRA,
};
use crate::test_manager::{TestManager, TestParameters, FUNDED_ACCOUNT_ADDRESS};
use crate::utils::{
    divide_vec_into_n_parts,
    get_class_hash_of_feature_contract,
    get_class_info_of_cairo0_contract,
    get_class_info_of_feature_contract,
};

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

#[rstest]
#[tokio::test]
async fn test_encrypted_state_diff(
    #[values(false, true)] use_kzg_da: bool,
    #[values(false, true)] full_output: bool,
    #[values(None, Some(vec![]), Some(vec![Felt::THREE, Felt::ONE]))] private_keys: Option<
        Vec<Felt>,
    >,
) {
    let (mut test_manager, [test_contract_address]) =
        TestManager::<DictStateReader>::new_with_default_initial_state([(
            FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
            calldata![Felt::ONE, Felt::TWO],
        )])
        .await;

    // Invoke a function on the test contract that changes the storage.
    let (key, value) = (Felt::from(10u8), Felt::from(11u8));
    let calldata = create_calldata(test_contract_address, "test_storage_read_write", &[key, value]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Run the test and assert the diff is as expected.
    let test_output = test_manager
        .execute_test_with_default_block_contexts(&TestParameters {
            use_kzg_da,
            full_output,
            private_keys,
            ..Default::default()
        })
        .await;
    let perform_global_validations = true;
    let partial_state_diff = StateDiff {
        storage_updates: HashMap::from([(
            test_contract_address,
            HashMap::from([(
                StarknetStorageKey(key.try_into().unwrap()),
                StarknetStorageValue(value),
            )]),
        )]),
        ..Default::default()
    };
    test_output.perform_validations(perform_global_validations, Some(&partial_state_diff));
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

#[rstest]
#[tokio::test]
async fn test_os_logic(
    #[values(1, 3)] n_blocks_in_multi_block: usize,
    #[values(None, Some(vec![Felt::ONE, Felt::TWO]))] private_keys: Option<Vec<Felt>>,
) {
    let (mut test_manager, _) =
        TestManager::<DictStateReader>::new_with_default_initial_state([]).await;
    let n_expected_txs = 29;
    let mut expected_storage_updates: HashMap<
        ContractAddress,
        HashMap<StarknetStorageKey, StarknetStorageValue>,
    > = HashMap::new();
    let mut update_expected_storage = |address: ContractAddress, key: Felt, value: Felt| {
        let key = StarknetStorageKey(StorageKey(key.try_into().unwrap()));
        let value = StarknetStorageValue(value);
        expected_storage_updates
            .entry(address)
            .and_modify(|map| {
                map.insert(key, value);
            })
            .or_insert_with(|| HashMap::from([(key, value)]));
    };

    // Declare a Cairo 0 test contract.
    let cairo0_test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let test_class_hash = get_class_hash_of_feature_contract(cairo0_test_contract);
    let declare_args = declare_tx_args! {
        version: TransactionVersion::ZERO,
        max_fee: Fee(1_000_000_000_000_000),
        class_hash: ClassHash(test_class_hash.0),
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
    };
    let account_declare_tx = declare_tx(declare_args);
    let class_info = get_class_info_of_feature_contract(cairo0_test_contract);
    let tx =
        DeclareTransaction::create(account_declare_tx, class_info, &CHAIN_ID_FOR_TESTS).unwrap();
    test_manager
        .add_cairo0_declare_tx(tx, get_class_hash_of_feature_contract(cairo0_test_contract));

    // Deploy some instances of the deprecated (cairo0) test contract.
    let mut contract_addresses = Vec::new();
    for (salt, ctor_calldata) in
        std::iter::zip([17u8, 42, 53], [[321u16, 543], [111, 987], [444, 0]])
    {
        let contract_address_salt = ContractAddressSalt(Felt::from(salt));
        let (deploy_tx, address) = get_deploy_contract_tx_and_address_with_salt(
            test_class_hash,
            Calldata(Arc::new(ctor_calldata.into_iter().map(Felt::from).collect())),
            test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
            *NON_TRIVIAL_RESOURCE_BOUNDS,
            contract_address_salt,
        );
        contract_addresses.push(address);
        test_manager.add_invoke_tx(deploy_tx, None);
        // Update expected storage diff, if the ctor calldata writes a nonzero value.
        if ctor_calldata[1] != 0 {
            update_expected_storage(
                address,
                Felt::from(ctor_calldata[0]),
                Felt::from(ctor_calldata[1]),
            );
        }
    }

    // Call set_value(address=85, value=47) on the first contract.
    // Used to test normal value update and make sure it is written to on-chain data.
    let (key, value) = (Felt::from(85), Felt::from(47));
    let calldata = create_calldata(contract_addresses[0], "test_storage_read_write", &[key, value]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    update_expected_storage(contract_addresses[0], key, value);

    // Call set_value(address=81, value=0) on the first contract.
    // Used to test redundant value update (0 -> 0) and make sure it is not written to on-chain
    // data.
    let calldata = create_calldata(
        contract_addresses[0],
        "test_storage_read_write",
        &[Felt::from(81), Felt::ZERO],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call set_value(address=97, value=0).
    // Used to test redundant value update (0 -> 0) in contract with only redundant updates
    // and make sure the whole contract is not written to on-chain data.
    let calldata = create_calldata(
        contract_addresses[2],
        "test_storage_read_write",
        &[Felt::from(97), Felt::ZERO],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    let calldata = create_calldata(contract_addresses[1], "read_write_read", &[]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    update_expected_storage(contract_addresses[1], Felt::from(15), Felt::ONE);

    let calldata = create_calldata(contract_addresses[0], "test_builtins", &[]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_get_block_timestamp with the current (testing) block timestamp.
    let calldata = create_calldata(
        contract_addresses[1],
        "test_get_block_timestamp",
        &[Felt::from(CURRENT_BLOCK_TIMESTAMP)],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // TODO(Yoni): test the effect of the event emission on the block hash, once calculated in the
    //   OS.
    let calldata = create_calldata(
        contract_addresses[1],
        "test_emit_events",
        // n_events, keys_len, keys, data_len, data.
        &[Felt::ONE, Felt::ONE, Felt::from(1991), Felt::ONE, Felt::from(2021)],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Calculate the block number of the next transaction.
    let txs_per_block = n_expected_txs / n_blocks_in_multi_block;
    // Calculate the block number of tx 'len(txs) + 1' without the added empty block.
    let mut block_number_offset = (test_manager.total_txs() + 1) / txs_per_block;
    if block_number_offset * txs_per_block < test_manager.total_txs() + 1 {
        block_number_offset += 1;
    }
    // If the block number is the last then we added an empty block before this block so its
    // block number is `n_blocks_in_multi_block + 1`
    if n_blocks_in_multi_block > 1 && block_number_offset == n_blocks_in_multi_block {
        block_number_offset = n_blocks_in_multi_block + 1;
    }

    // Call test_get_block_number(expected_block_number).
    let expected_block_number = test_manager.initial_state.next_block_number.0 - 1
        + u64::try_from(block_number_offset).unwrap();
    let calldata = create_calldata(
        contract_addresses[0],
        "test_get_block_number",
        &[Felt::from(expected_block_number)],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call contract -> send message to L1.
    let inner_calldata = [Felt::from(85)];
    let calldata = create_calldata(
        contract_addresses[0],
        "test_call_contract",
        &[
            **contract_addresses[0],
            selector_from_name("send_message").0,
            inner_calldata.len().into(),
            inner_calldata[0],
        ],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    let expected_message_to_l1 = MessageToL1 {
        from_address: contract_addresses[0],
        to_address: EthAddress::try_from(Felt::from(85)).unwrap(),
        payload: L2ToL1Payload(vec![Felt::from(12), Felt::from(34)]),
    };

    // Test get_caller_address syscall.
    let calldata = create_calldata(
        contract_addresses[0],
        "test_call_contract",
        &[
            *contract_addresses[1].0.key(),
            selector_from_name("test_get_caller_address").0,
            Felt::ONE,               // Inner calldata length.
            **contract_addresses[0], // Expected caller address.
        ],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    let calldata = create_calldata(
        contract_addresses[0],
        "test_get_contract_address",
        &[**contract_addresses[0]],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Delegate proxy tests.

    let delegate_proxy_contract = FeatureContract::DelegateProxy;

    // Declare and deploy the delegate proxy contract.
    let delegate_proxy_class_hash = get_class_hash_of_feature_contract(delegate_proxy_contract);
    let delegate_proxy_declare_tx = declare_tx(declare_tx_args! {
        version: TransactionVersion::ZERO,
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        class_hash: delegate_proxy_class_hash,
        max_fee: Fee(1_000_000_000_000_000),
    });
    let class_info = get_class_info_of_feature_contract(delegate_proxy_contract);
    let tx = DeclareTransaction::create(delegate_proxy_declare_tx, class_info, &CHAIN_ID_FOR_TESTS)
        .unwrap();
    test_manager.add_cairo0_declare_tx(tx, delegate_proxy_class_hash);

    let contract_address_salt = ContractAddressSalt(Felt::ZERO);
    let (deploy_tx, delegate_proxy_address) = get_deploy_contract_tx_and_address_with_salt(
        delegate_proxy_class_hash,
        Calldata::default(),
        test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        *NON_TRIVIAL_RESOURCE_BOUNDS,
        contract_address_salt,
    );
    test_manager.add_invoke_tx(deploy_tx, None);

    // Set implementation to the test contract.
    let calldata =
        create_calldata(delegate_proxy_address, "set_implementation_hash", &[test_class_hash.0]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    update_expected_storage(
        delegate_proxy_address,
        **get_storage_var_address("implementation_hash", &[]),
        test_class_hash.0,
    );

    // Call test_get_contract_address(expected_address=delegate_proxy_address) through the delegate
    // proxy.
    let calldata = create_calldata(
        delegate_proxy_address,
        "test_get_contract_address",
        &[**delegate_proxy_address],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call set_value(address=123, value=456) through the delegate proxy.
    let (key, value) = (Felt::from(123), Felt::from(456));
    let calldata =
        create_calldata(delegate_proxy_address, "test_storage_read_write", &[key, value]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    update_expected_storage(delegate_proxy_address, key, value);

    // Call test_get_caller_address(expected_address=account_address) through the delegate proxy.
    let calldata = create_calldata(
        delegate_proxy_address,
        "test_get_caller_address",
        &[***FUNDED_ACCOUNT_ADDRESS],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // call_contract -> get_sequencer_address.
    let calldata = create_calldata(
        delegate_proxy_address,
        "test_call_contract",
        &[
            **delegate_proxy_address,
            selector_from_name("test_get_sequencer_address").0,
            Felt::ONE,
            Felt::from_hex_unchecked(TEST_SEQUENCER_ADDRESS),
        ],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Invoke the l1_handler deposit(from_address=85, amount=2) through the delegate proxy, and
    // define the expected consumed message.
    let l1_handler_calldata = calldata![Felt::from(85), Felt::TWO];
    let l1_handler_nonce = Nonce::default();
    let l1_handler_selector = selector_from_name("deposit");
    let tx = ExecutableL1HandlerTransaction::create(
        L1HandlerTransaction {
            version: L1HandlerTransaction::VERSION,
            nonce: l1_handler_nonce,
            contract_address: delegate_proxy_address,
            entry_point_selector: l1_handler_selector,
            calldata: l1_handler_calldata.clone(),
        },
        &CHAIN_ID_FOR_TESTS,
        Fee(1_000_000),
    )
    .unwrap();
    test_manager.add_l1_handler_tx(tx, None);
    let expected_message_to_l2 = MessageToL2 {
        from_address: EthAddress::try_from(l1_handler_calldata.0[0]).unwrap(),
        to_address: delegate_proxy_address,
        payload: L1ToL2Payload(l1_handler_calldata.0[1..].to_vec()),
        nonce: l1_handler_nonce,
        selector: l1_handler_selector,
    };
    update_expected_storage(
        delegate_proxy_address,
        **get_storage_var_address(
            "two_counters",
            &[Felt::from(expected_message_to_l2.from_address)],
        ),
        *expected_message_to_l2.payload.0.last().unwrap(),
    );

    // Call test_library_call_syntactic_sugar from contract_addresses[0] to test library calls
    // using the syntactic sugar of 'library_call_<FUNCTION>'.
    let calldata = create_calldata(
        contract_addresses[0],
        "test_library_call_syntactic_sugar",
        &[test_class_hash.0],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    update_expected_storage(contract_addresses[0], Felt::from(444), Felt::from(666));

    // Call add_signature_to_counters(index=2021).
    let index = Felt::from(2021);
    let calldata = create_calldata(contract_addresses[0], "add_signature_to_counters", &[index]);
    let signature = TransactionSignature(Arc::new(vec![Felt::from(100), Felt::from(200)]));
    test_manager
        .add_funded_account_invoke(invoke_tx_args! { calldata, signature: signature.clone() });
    update_expected_storage(
        contract_addresses[0],
        **get_storage_var_address("two_counters", &[index]),
        signature.0[0],
    );
    update_expected_storage(
        contract_addresses[0],
        **get_storage_var_address("two_counters", &[index]) + Felt::ONE,
        signature.0[1],
    );

    // Declare test_contract2.
    let test_contract2 = FeatureContract::TestContract2;
    let test_contract2_class_hash = get_class_hash_of_feature_contract(test_contract2);
    let test_contract2_declare_tx = declare_tx(declare_tx_args! {
        version: TransactionVersion::ZERO,
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        class_hash: test_contract2_class_hash,
        max_fee: Fee(1_000_000_000_000_000),
    });
    let class_info = get_class_info_of_feature_contract(test_contract2);
    let tx = DeclareTransaction::create(test_contract2_declare_tx, class_info, &CHAIN_ID_FOR_TESTS)
        .unwrap();
    test_manager.add_cairo0_declare_tx(tx, test_contract2_class_hash);

    // Use library_call to call test_contract2.test_storage_write(address=555, value=888).
    let (key, value) = (Felt::from(555), Felt::from(888));
    let calldata = create_calldata(
        contract_addresses[1],
        "test_library_call",
        &[
            test_contract2_class_hash.0,
            selector_from_name("test_storage_write").0,
            Felt::TWO,
            key,
            value,
        ],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    update_expected_storage(contract_addresses[1], key, value);

    // Use library_call_l1_handler to invoke test_contract2.test_l1_handler_storage_write with
    // from_address=85, address=666, value=999.
    let (key, value) = (Felt::from(666), Felt::from(999));
    let calldata = create_calldata(
        contract_addresses[1],
        "test_library_call_l1_handler",
        &[
            test_contract2_class_hash.0,
            selector_from_name("test_l1_handler_storage_write").0,
            Felt::THREE,
            Felt::from(85),
            key,
            value,
        ],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    update_expected_storage(contract_addresses[1], key, value);

    // Replace the class of contract_addresses[0] to the class of test_contract2.
    let calldata = create_calldata(
        contract_addresses[0],
        "test_replace_class",
        &[test_contract2_class_hash.0],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Expected number of txs.
    assert_eq!(test_manager.total_txs(), n_expected_txs);

    // Run the test.
    test_manager.divide_transactions_into_n_blocks(n_blocks_in_multi_block);
    let test_output = test_manager
        .execute_test_with_default_block_contexts(&TestParameters {
            messages_to_l1: vec![expected_message_to_l1],
            messages_to_l2: vec![expected_message_to_l2],
            private_keys,
            ..Default::default()
        })
        .await;

    // Perform validations.
    let perform_global_validations = true;
    let partial_state_diff =
        Some(&StateDiff { storage_updates: expected_storage_updates, ..Default::default() });
    test_output.perform_validations(perform_global_validations, partial_state_diff);
}

#[rstest]
#[tokio::test]
async fn test_v1_bound_accounts_cairo0() {
    let test_contract = &V1_BOUND_CAIRO0_CONTRACT;
    let class_hash = ClassHash(compute_deprecated_class_hash(test_contract).unwrap());
    let vc = VersionedConstants::latest_constants();
    let (mut test_manager, _) =
        TestManager::<DictStateReader>::new_with_default_initial_state([]).await;

    assert!(vc.os_constants.v1_bound_accounts_cairo0.contains(&class_hash));

    // Declare the V1-bound account.
    let declare_args = declare_tx_args! {
        version: TransactionVersion::ZERO,
        max_fee: Fee(1_000_000_000_000_000),
        class_hash,
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
    };
    let account_declare_tx = declare_tx(declare_args);
    let class_info = get_class_info_of_cairo0_contract((**test_contract).clone());
    let tx =
        DeclareTransaction::create(account_declare_tx, class_info, &CHAIN_ID_FOR_TESTS).unwrap();
    test_manager.add_cairo0_declare_tx(tx, class_hash);

    // Deploy it.
    let salt = ContractAddressSalt(Felt::ZERO);
    let (deploy_tx, v1_bound_account_address) = get_deploy_contract_tx_and_address_with_salt(
        class_hash,
        Calldata::default(),
        test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        *NON_TRIVIAL_RESOURCE_BOUNDS,
        salt,
    );
    test_manager.add_invoke_tx(deploy_tx, None);

    // Initialize the account.
    let private_key = Felt::ONE;
    let public_key = get_public_key(&private_key);
    let guardian = Felt::ZERO;
    let calldata = create_calldata(v1_bound_account_address, "initialize", &[public_key, guardian]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Create a validate tx and add signature to the transaction. The dummy account used to call
    // `__validate__` does not check the signature, so we can use the signature field for
    // `__validate__`. This is done after creating the transaction so that we will have access
    // to the transaction hash.
    let validate_tx_args = invoke_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        nonce: test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        calldata: create_calldata(
            v1_bound_account_address, "__validate__", &[Felt::ZERO, Felt::ZERO]
        ),
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        tip: vc.os_constants.v1_bound_accounts_max_tip,
    };
    let validate_tx =
        InvokeTransaction::create(invoke_tx(validate_tx_args.clone()), &CHAIN_ID_FOR_TESTS)
            .unwrap();
    assert_eq!(validate_tx.version(), TransactionVersion::THREE);
    let Signature { r, s } = ecdsa_sign(&private_key, &validate_tx.tx_hash()).unwrap().into();
    let validate_tx_args = invoke_tx_args! {
        signature: TransactionSignature(Arc::new(vec![r, s])),
        ..validate_tx_args
    };
    test_manager.add_invoke_tx_from_args(validate_tx_args, &CHAIN_ID_FOR_TESTS, None);

    // Run test and verify the signer was set.
    let test_output =
        test_manager.execute_test_with_default_block_contexts(&TestParameters::default()).await;

    let expected_storage_updates = HashMap::from([(
        v1_bound_account_address,
        HashMap::from([(
            StarknetStorageKey(get_storage_var_address("_signer", &[])),
            StarknetStorageValue(public_key),
        )]),
    )]);
    let perform_global_validations = true;
    let partial_state_diff =
        Some(&StateDiff { storage_updates: expected_storage_updates, ..Default::default() });
    test_output.perform_validations(perform_global_validations, partial_state_diff);
}

#[rstest]
#[tokio::test]
async fn test_v1_bound_accounts_cairo1() {
    let test_contract_sierra = &V1_BOUND_CAIRO1_CONTRACT_SIERRA;
    let _test_contract_casm = &V1_BOUND_CAIRO1_CONTRACT_CASM;
    let class_hash = test_contract_sierra.calculate_class_hash();
    let vc = VersionedConstants::latest_constants();
    assert!(vc.os_constants.v1_bound_accounts_cairo1.contains(&class_hash));

    // TODO(Dori): Impl the test.
}
