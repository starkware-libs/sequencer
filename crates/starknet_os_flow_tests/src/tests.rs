use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::transaction::test_utils::ExpectedExecutionInfo;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::types::builtin_name::BuiltinName;
use expect_test::expect;
use rstest::rstest;
use starknet_api::abi::abi_utils::{get_storage_var_address, selector_from_name};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::{ClassInfo, ContractClass};
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    CompiledClassHash as StarknetAPICompiledClassHash,
    ContractAddress,
    EthAddress,
    Nonce,
    PatriciaKey,
};
use starknet_api::executable_transaction::{
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction as ExecutableL1HandlerTransaction,
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::StorageKey;
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::deploy_account::deploy_account_tx;
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
    Tip,
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
use starknet_api::{
    calldata,
    contract_address,
    declare_tx_args,
    deploy_account_tx_args,
    felt,
    invoke_tx_args,
};
use starknet_committer::block_committer::input::{
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_core::crypto::ecdsa_sign;
use starknet_crypto::{get_public_key, Signature};
use starknet_os::hints::hint_implementation::deprecated_compiled_class::class_hash::compute_deprecated_class_hash;
use starknet_os::hints::vars::Const;
use starknet_os::io::os_output::MessageToL2;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Pedersen, StarkHash};

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
    maybe_dummy_block_hash_and_number,
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
    test_output.perform_default_validations();
}

#[rstest]
#[tokio::test]
async fn test_os_logic(#[values(1, 3)] n_blocks_in_multi_block: usize) {
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
    test_manager.add_cairo0_declare_tx(
        tx,
        StarknetAPICompiledClassHash(get_class_hash_of_feature_contract(cairo0_test_contract).0),
    );

    // Deploy some instances of the test contract.
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
            contract_address_salt.0,
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

    let calldata = create_calldata(contract_addresses[1], "entry_point", &[]);
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
    let calldata = create_calldata(
        contract_addresses[0],
        "test_call_contract",
        &[**contract_addresses[0], selector_from_name("send_message").0, Felt::ONE, Felt::from(85)],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    let expected_message_to_l1 = MessageToL1 {
        from_address: contract_addresses[0],
        to_address: EthAddress::try_from(Felt::from(85)).unwrap(),
        payload: L2ToL1Payload(vec![Felt::from(12), Felt::from(34)]),
    };

    // Test get_contract_address.
    let calldata = create_calldata(
        contract_addresses[0],
        "test_call_contract",
        &[
            *contract_addresses[1].0.key(),
            selector_from_name("test_get_caller_address").0,
            Felt::ONE,
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
    test_manager
        .add_cairo0_declare_tx(tx, StarknetAPICompiledClassHash(delegate_proxy_class_hash.0));

    let contract_address_salt = ContractAddressSalt(Felt::ZERO);
    let (deploy_tx, delegate_proxy_address) = get_deploy_contract_tx_and_address_with_salt(
        delegate_proxy_class_hash,
        Calldata::default(),
        test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        *NON_TRIVIAL_RESOURCE_BOUNDS,
        contract_address_salt.0,
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
    test_manager
        .add_cairo0_declare_tx(tx, StarknetAPICompiledClassHash(test_contract2_class_hash.0));

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
    let signature = TransactionSignature(Arc::new(vec![Felt::from(100)]));
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata, signature });
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
    let signature = TransactionSignature(Arc::new(vec![Felt::from(100)]));
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata, signature });
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
    test_manager.add_cairo0_declare_tx(tx, StarknetAPICompiledClassHash(class_hash.0));

    // Deploy it.
    let salt = Felt::ZERO;
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
    let test_contract_casm = &V1_BOUND_CAIRO1_CONTRACT_CASM;
    let class_hash = test_contract_sierra.calculate_class_hash();
    let compiled_class_hash = test_contract_casm.hash(&HashVersion::V2);
    let vc = VersionedConstants::latest_constants();
    let max_tip = vc.os_constants.v1_bound_accounts_max_tip;
    assert!(vc.os_constants.v1_bound_accounts_cairo1.contains(&class_hash));
    let (mut test_manager, _) =
        TestManager::<DictStateReader>::new_with_default_initial_state([]).await;

    // Declare the V1-bound account.
    let declare_args = declare_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        nonce: test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        class_hash,
        compiled_class_hash,
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
    };
    let account_declare_tx = declare_tx(declare_args);
    let sierra_version = test_contract_sierra.get_sierra_version().unwrap();
    let class_info = ClassInfo {
        contract_class: ContractClass::V1(((**test_contract_casm).clone(), sierra_version.clone())),
        sierra_program_length: test_contract_sierra.sierra_program.len(),
        abi_length: test_contract_sierra.abi.len(),
        sierra_version,
    };
    let tx =
        DeclareTransaction::create(account_declare_tx, class_info, &CHAIN_ID_FOR_TESTS).unwrap();
    test_manager.add_cairo1_declare_tx(tx, test_contract_sierra);

    // Deploy it (from funded account).
    let private_key = Felt::ONE;
    let public_key = get_public_key(&private_key);
    let salt = Felt::ZERO;
    let (deploy_tx, v1_bound_account_address) = get_deploy_contract_tx_and_address_with_salt(
        class_hash,
        Calldata(Arc::new(vec![public_key])),
        test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        *NON_TRIVIAL_RESOURCE_BOUNDS,
        salt,
    );
    test_manager.add_invoke_tx(deploy_tx, None);

    // Transfer funds to the account.
    let transfer_amount = 2 * NON_TRIVIAL_RESOURCE_BOUNDS.max_possible_fee(max_tip).0;
    test_manager.add_fund_address_tx(v1_bound_account_address, transfer_amount);

    // Create an invoke tx, compute the hash, sign the hash and update the signature on the tx.
    let invoke_tx_args = invoke_tx_args! {
        sender_address: v1_bound_account_address,
        nonce: test_manager.next_nonce(v1_bound_account_address),
        calldata: Calldata(Arc::new(vec![Felt::ZERO])),
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
    };
    let invoke_tx =
        InvokeTransaction::create(invoke_tx(invoke_tx_args.clone()), &CHAIN_ID_FOR_TESTS).unwrap();
    assert_eq!(invoke_tx.version(), TransactionVersion::THREE);
    let Signature { r, s } = ecdsa_sign(&private_key, &invoke_tx.tx_hash()).unwrap().into();
    let invoke_tx_args = invoke_tx_args! {
        signature: TransactionSignature(Arc::new(vec![r, s])),
        ..invoke_tx_args
    };
    test_manager.add_invoke_tx_from_args(invoke_tx_args, &CHAIN_ID_FOR_TESTS, None);

    // Run the test, and make sure the account storage has the expected changes.
    let test_output =
        test_manager.execute_test_with_default_block_contexts(&TestParameters::default()).await;
    let isrc6_id = Felt::from_hex_unchecked(
        "0x2CECCEF7F994940B3962A6C67E0BA4FCD37DF7D131417C604F91E03CAECC1CD",
    );
    let expected_storage_updates = HashMap::from([(
        v1_bound_account_address,
        HashMap::from([
            (
                StarknetStorageKey(selector_from_name("Account_public_key").0.try_into().unwrap()),
                StarknetStorageValue(public_key),
            ),
            (
                StarknetStorageKey(
                    Pedersen::hash(&selector_from_name("SRC5_supported_interfaces").0, &isrc6_id)
                        .try_into()
                        .unwrap(),
                ),
                StarknetStorageValue(Felt::ONE),
            ),
        ]),
    )]);
    let perform_global_validations = true;
    let partial_state_diff =
        Some(&StateDiff { storage_updates: expected_storage_updates, ..Default::default() });
    test_output.perform_validations(perform_global_validations, partial_state_diff);
}

#[rstest]
#[case::use_kzg(true, 5)]
#[case::not_use_kzg(false, 1)]
#[tokio::test]
async fn test_new_class_flow(#[case] use_kzg_da: bool, #[case] n_blocks_in_multi_block: usize) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let test_class_hash = get_class_hash_of_feature_contract(test_contract);
    let (mut test_manager, [main_contract_address]) =
        TestManager::<DictStateReader>::new_with_default_initial_state([(
            test_contract,
            calldata![Felt::ZERO, Felt::ZERO],
        )])
        .await;
    let current_block_number = test_manager.initial_state.next_block_number;

    assert!(
        current_block_number.0 > STORED_BLOCK_HASH_BUFFER,
        "Current block number must be greater than STORED_BLOCK_HASH_BUFFER for the test to work."
    );

    // Prepare expected storage updates.
    let mut expected_messages_to_l1 = Vec::new();
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

    // Call test_increment twice.
    let n_calls: u8 = 2;
    for _ in 0..n_calls {
        let calldata = create_calldata(
            main_contract_address,
            "test_increment",
            &[felt!(5u8), felt!(6u8), felt!(7u8)],
        );
        test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    }
    update_expected_storage(
        main_contract_address,
        **get_storage_var_address("my_storage_var", &[]),
        Felt::from(n_calls),
    );

    // Test get_execution_info; invoke a function that gets the expected execution info and compares
    // it to the actual.
    let test_execution_info_selector_name = "test_get_execution_info";
    let test_execution_info_selector = selector_from_name(test_execution_info_selector_name);
    let only_query = false;
    let expected_execution_info = ExpectedExecutionInfo::new(
        only_query,
        *FUNDED_ACCOUNT_ADDRESS,
        *FUNDED_ACCOUNT_ADDRESS,
        main_contract_address,
        CHAIN_ID_FOR_TESTS.clone(),
        test_execution_info_selector,
        current_block_number,
        BlockTimestamp(CURRENT_BLOCK_TIMESTAMP),
        contract_address!(TEST_SEQUENCER_ADDRESS),
        *NON_TRIVIAL_RESOURCE_BOUNDS,
        test_manager.get_nonce(*FUNDED_ACCOUNT_ADDRESS),
    )
    .to_syscall_result();
    let invoke_tx_args = invoke_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        nonce: test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        calldata: create_calldata(
            main_contract_address, test_execution_info_selector_name, &expected_execution_info
        ),
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
    };
    // Put the tx hash in the signature.
    let tx =
        InvokeTransaction::create(invoke_tx(invoke_tx_args.clone()), &CHAIN_ID_FOR_TESTS).unwrap();
    test_manager.add_invoke_tx_from_args(
        invoke_tx_args! {
            signature: TransactionSignature(Arc::new(vec![tx.tx_hash.0])),
            ..invoke_tx_args
        },
        &CHAIN_ID_FOR_TESTS,
        None,
    );

    // Test Cairo 1.0 deploy syscall.
    let salt = Felt::from(7);
    let deploy_from_zero = Felt::ZERO;
    let ctor_calldata = vec![Felt::ZERO, Felt::ZERO];
    let calldata = create_calldata(
        main_contract_address,
        "test_deploy",
        &[
            test_class_hash.0,
            salt,
            ctor_calldata.len().into(),
            ctor_calldata[0],
            ctor_calldata[1],
            deploy_from_zero,
        ],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata, tip: Tip(1234) });
    let contract_address2 = calculate_contract_address(
        ContractAddressSalt(salt),
        test_class_hash,
        &Calldata(Arc::new(ctor_calldata)),
        main_contract_address,
    )
    .unwrap();

    // Test calling test_get_execution_info.
    let test_call_contract_selector_name = "test_call_contract";
    let expected_execution_info = ExpectedExecutionInfo::new(
        only_query,
        *FUNDED_ACCOUNT_ADDRESS,
        main_contract_address,
        contract_address2,
        CHAIN_ID_FOR_TESTS.clone(),
        test_execution_info_selector,
        current_block_number,
        BlockTimestamp(CURRENT_BLOCK_TIMESTAMP),
        contract_address!(TEST_SEQUENCER_ADDRESS),
        *NON_TRIVIAL_RESOURCE_BOUNDS,
        test_manager.get_nonce(*FUNDED_ACCOUNT_ADDRESS),
    )
    .to_syscall_result();
    let invoke_tx_args = invoke_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        nonce: test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        calldata: create_calldata(
            main_contract_address,
            test_call_contract_selector_name,
            &[
                **contract_address2,
                test_execution_info_selector.0,
                expected_execution_info.len().into()
            ].into_iter().chain(expected_execution_info.into_iter()).collect::<Vec<_>>()
        ),
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
    };
    // Put the tx hash in the signature.
    let invoke_tx =
        InvokeTransaction::create(invoke_tx(invoke_tx_args.clone()), &CHAIN_ID_FOR_TESTS).unwrap();
    test_manager.add_invoke_tx_from_args(
        invoke_tx_args! {
            signature: TransactionSignature(Arc::new(vec![invoke_tx.tx_hash.0])),
            ..invoke_tx_args
        },
        &CHAIN_ID_FOR_TESTS,
        None,
    );

    // Test calling test_storage_read_write.
    let test_call_contract_key = Felt::from(1948);
    let test_call_contract_value = Felt::from(1967);
    let calldata = create_calldata(
        main_contract_address,
        test_call_contract_selector_name,
        &[
            **contract_address2,
            selector_from_name("test_storage_read_write").0,
            Felt::TWO,
            test_call_contract_key,
            test_call_contract_value,
        ],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    update_expected_storage(contract_address2, test_call_contract_key, test_call_contract_value);

    // Test the behavior of the `get_class_hash_at` syscall.
    let deployed_address = contract_address2;
    let expected_class_hash_of_deployed_address = test_class_hash;
    let undeployed_address = Felt::from(123456789);
    let calldata = create_calldata(
        main_contract_address,
        test_call_contract_selector_name,
        &[
            **contract_address2,
            selector_from_name("test_get_class_hash_at").0,
            Felt::THREE,
            **deployed_address,
            expected_class_hash_of_deployed_address.0,
            undeployed_address,
        ],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Test send-message-to-L1 syscall.
    let test_send_message_to_l1_to_address = Felt::ZERO;
    let test_send_message_to_l1_payload = vec![Felt::from(4365), Felt::from(23)];
    let calldata = create_calldata(
        main_contract_address,
        test_call_contract_selector_name,
        &[
            **contract_address2,
            selector_from_name("test_send_message_to_l1").0,
            Felt::from(4),
            test_send_message_to_l1_to_address,
            Felt::TWO,
            test_send_message_to_l1_payload[0],
            test_send_message_to_l1_payload[1],
        ],
    );
    expected_messages_to_l1.push(MessageToL1 {
        from_address: contract_address2,
        to_address: EthAddress::try_from(test_send_message_to_l1_to_address).unwrap(),
        payload: L2ToL1Payload(test_send_message_to_l1_payload),
    });
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_poseidon_hades_permutation.
    let calldata = create_calldata(main_contract_address, "test_poseidon_hades_permutation", &[]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_keccak.
    let calldata = create_calldata(main_contract_address, "test_keccak", &[]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Points for keccak / secp tests.
    let x_low = Felt::from(302934307671667531413257853548643485645u128);
    let x_high = Felt::from(328530677494498397859470651507255972949u128);
    let y_low = Felt::from(11797905874978945418374634252637373969u128);
    let y_high = Felt::from(188875896373816311474931247321846847606u128);

    // Call test_keccak.
    let calldata =
        create_calldata(main_contract_address, "test_new_point_secp256k1", &[x_low, x_high]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_getter_secp256k1.
    let calldata = create_calldata(
        main_contract_address,
        "test_getter_secp256k1",
        &[x_low, x_high, y_low, y_high],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_add_secp256k1.
    let calldata = create_calldata(
        main_contract_address,
        "test_add_secp256k1",
        &[x_low, x_high, y_low, y_high],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_mul_secp256k1.
    let calldata = create_calldata(
        main_contract_address,
        "test_mul_secp256k1",
        &[Felt::from(1991), Felt::from(1996)],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_signature_verification_secp256k1.
    let calldata =
        create_calldata(main_contract_address, "test_signature_verification_secp256k1", &[]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Change points for secpR.
    let x_low = Felt::from_hex_unchecked("0x2D483FE223B12B91047D83258A958B0F");
    let x_high = Felt::from_hex_unchecked("0x502A43CE77C6F5C736A82F847FA95F8C");
    let y_low = Felt::from_hex_unchecked("0xCE729C7704F4DDF2EAAF0B76209FE1B0");
    let y_high = Felt::from_hex_unchecked("0xDB0A2E6710C71BA80AFEB3ABDF69D306");

    // Call test_new_point_secp256r1.
    let calldata =
        create_calldata(main_contract_address, "test_new_point_secp256r1", &[x_low, x_high]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_add_secp256r1.
    let calldata = create_calldata(
        main_contract_address,
        "test_add_secp256r1",
        &[x_low, x_high, y_low, y_high],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_getter_secp256r1.
    let calldata = create_calldata(
        main_contract_address,
        "test_getter_secp256r1",
        &[x_low, x_high, y_low, y_high],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_mul_secp256r1.
    let calldata = create_calldata(
        main_contract_address,
        "test_mul_secp256r1",
        &[Felt::from(1991), Felt::from(1996)],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_signature_verification_secp256r1.
    let calldata =
        create_calldata(main_contract_address, "test_signature_verification_secp256r1", &[]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Declare the experimental contract.
    let experimental_contract = FeatureContract::Experimental;
    let experimental_contract_sierra = experimental_contract.get_sierra();
    let experimental_class_hash = experimental_contract_sierra.calculate_class_hash();
    let experimental_compiled_class_hash =
        experimental_contract.get_compiled_class_hash(&HashVersion::V2);
    let declare_tx_args = declare_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        class_hash: experimental_class_hash,
        compiled_class_hash: experimental_compiled_class_hash,
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        nonce: test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
    };
    let account_declare_tx = declare_tx(declare_tx_args);
    let class_info = get_class_info_of_feature_contract(experimental_contract);
    let tx =
        DeclareTransaction::create(account_declare_tx, class_info, &CHAIN_ID_FOR_TESTS).unwrap();
    test_manager.add_cairo1_declare_tx(tx, &experimental_contract_sierra);

    // Deploy it.
    let salt = Felt::ZERO;
    let (deploy_tx, _experimental_contract_address) = get_deploy_contract_tx_and_address_with_salt(
        experimental_class_hash,
        Calldata::default(),
        test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        *NON_TRIVIAL_RESOURCE_BOUNDS,
        salt,
    );
    test_manager.add_invoke_tx(deploy_tx, None);

    // Call test_sha256.
    let calldata = create_calldata(main_contract_address, "test_sha256", &[]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_circuit.
    let calldata = create_calldata(main_contract_address, "test_circuit", &[]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_rc96_holes.
    let calldata = create_calldata(main_contract_address, "test_rc96_holes", &[]);
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Call test_get_block_hash.
    // Get the block hash at current block number minus STORED_BLOCK_HASH_BUFFER.
    let queried_block_number = BlockNumber(current_block_number.0 - STORED_BLOCK_HASH_BUFFER);
    let (_old_block_number, old_block_hash) =
        maybe_dummy_block_hash_and_number(current_block_number).unwrap();
    let calldata = create_calldata(
        main_contract_address,
        "test_get_block_hash",
        &[Felt::from(queried_block_number.0), old_block_hash.0],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });

    // Test library call.
    // TODO(Yoni): test execution info on library call.
    let test_library_call_key = Felt::from(1973);
    let test_library_call_value = Felt::from(1982);
    let calldata = create_calldata(
        main_contract_address,
        "test_library_call",
        &[
            test_class_hash.0,
            selector_from_name("test_storage_read_write").0,
            Felt::TWO,
            test_library_call_key,
            test_library_call_value,
        ],
    );
    test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    update_expected_storage(main_contract_address, test_library_call_key, test_library_call_value);

    // Test segment_arena.
    for _ in 0..2 {
        let calldata = create_calldata(main_contract_address, "test_segment_arena", &[]);
        test_manager.add_funded_account_invoke(invoke_tx_args! { calldata });
    }

    // Declare a Cairo 1.0 account contract.
    // TODO(Noa): Replace the main account of the test with this Cairo 1 account.
    let faulty_account = FeatureContract::FaultyAccount(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let faulty_account_sierra = faulty_account.get_sierra();
    let faulty_account_class_hash = faulty_account_sierra.calculate_class_hash();
    let faulty_account_compiled_class_hash =
        faulty_account.get_compiled_class_hash(&HashVersion::V2);
    let declare_tx_args = declare_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        class_hash: faulty_account_class_hash,
        compiled_class_hash: faulty_account_compiled_class_hash,
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        nonce: test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
    };
    let account_declare_tx = declare_tx(declare_tx_args);
    let class_info = get_class_info_of_feature_contract(faulty_account);
    let tx =
        DeclareTransaction::create(account_declare_tx, class_info, &CHAIN_ID_FOR_TESTS).unwrap();
    test_manager.add_cairo1_declare_tx(tx, &faulty_account_sierra);

    // Deploy it.
    let salt = Felt::ZERO;
    let validate_constructor = Felt::ZERO; // false.
    let ctor_calldata = calldata![validate_constructor];
    let (deploy_tx, _) = get_deploy_contract_tx_and_address_with_salt(
        faulty_account_class_hash,
        ctor_calldata.clone(),
        test_manager.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        *NON_TRIVIAL_RESOURCE_BOUNDS,
        salt,
    );
    test_manager.add_invoke_tx(deploy_tx, None);

    // Prepare deploying an instance of the account by precomputing the address and funding it.
    let valid = Felt::ZERO;
    let salt = Felt::from(1993);
    let faulty_account_address = calculate_contract_address(
        ContractAddressSalt(salt),
        faulty_account_class_hash,
        &ctor_calldata,
        ContractAddress::default(),
    )
    .unwrap();
    // Fund the address.
    test_manager.add_fund_address_tx_with_default_amount(faulty_account_address);

    // Create a DeployAccount transaction.
    let deploy_tx_args = deploy_account_tx_args! {
        class_hash: faulty_account_class_hash,
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        contract_address_salt: ContractAddressSalt(salt),
        signature: TransactionSignature(Arc::new(vec![valid])),
        constructor_calldata: ctor_calldata,
    };
    let deploy_account_tx =
        deploy_account_tx(deploy_tx_args, test_manager.next_nonce(faulty_account_address));
    test_manager.add_deploy_account_tx(
        DeployAccountTransaction::create(deploy_account_tx, &CHAIN_ID_FOR_TESTS).unwrap(),
    );

    // Declare a contract using the newly deployed account.
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let empty_contract_sierra = empty_contract.get_sierra();
    let empty_contract_class_hash = empty_contract_sierra.calculate_class_hash();
    let empty_contract_compiled_class_hash =
        empty_contract.get_compiled_class_hash(&HashVersion::V2);
    let declare_tx_args = declare_tx_args! {
        sender_address: faulty_account_address,
        class_hash: empty_contract_class_hash,
        compiled_class_hash: empty_contract_compiled_class_hash,
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        nonce: test_manager.next_nonce(faulty_account_address),
        signature: TransactionSignature(Arc::new(vec![valid])),
    };
    let account_declare_tx = declare_tx(declare_tx_args);
    let class_info = get_class_info_of_feature_contract(empty_contract);
    let tx =
        DeclareTransaction::create(account_declare_tx, class_info, &CHAIN_ID_FOR_TESTS).unwrap();
    test_manager.add_cairo1_declare_tx(tx, &empty_contract_sierra);
    // The faulty account's __execute__ sends a message to L1.
    expected_messages_to_l1.push(MessageToL1 {
        from_address: faulty_account_address,
        to_address: EthAddress::default(),
        payload: L2ToL1Payload::default(),
    });

    // Invoke a function on the new account.
    let invoke_tx_args = invoke_tx_args! {
        sender_address: faulty_account_address,
        nonce: test_manager.next_nonce(faulty_account_address),
        calldata: create_calldata(faulty_account_address, "foo", &[]),
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        signature: TransactionSignature(Arc::new(vec![valid])),
    };
    test_manager.add_invoke_tx_from_args(invoke_tx_args, &CHAIN_ID_FOR_TESTS, None);
    // The faulty account's __execute__ sends a message to L1.
    expected_messages_to_l1.push(MessageToL1 {
        from_address: faulty_account_address,
        to_address: EthAddress::default(),
        payload: L2ToL1Payload::default(),
    });

    // The OS is expected to write the (number -> hash) mapping of this block. Make sure the current
    // block number is greater than STORED_BLOCK_HASH_BUFFER.
    let old_block_number = current_block_number.0 - STORED_BLOCK_HASH_BUFFER;
    assert!(
        old_block_number > 0,
        "Block number must be big enough to test a non-trivial block hash mapping update."
    );

    // Add old block hashes to expected storage updates.
    let block_hash_contract_address = ContractAddress(
        PatriciaKey::try_from(Const::BlockHashContractAddress.fetch_from_os_program().unwrap())
            .unwrap(),
    );
    for block_number in current_block_number.0
        ..(current_block_number.0 + u64::try_from(n_blocks_in_multi_block).unwrap())
    {
        let (old_block_number, old_block_hash) =
            maybe_dummy_block_hash_and_number(BlockNumber(block_number)).unwrap();
        update_expected_storage(
            block_hash_contract_address,
            Felt::from(old_block_number.0),
            old_block_hash.0,
        );
    }

    // Run the test.
    test_manager.divide_transactions_into_n_blocks(n_blocks_in_multi_block);
    let test_output = test_manager
        .execute_test_with_default_block_contexts(&TestParameters {
            use_kzg_da,
            messages_to_l1: expected_messages_to_l1,
            ..Default::default()
        })
        .await;

    // Perform general validations and storage update validations.
    let perform_global_validations = true;
    test_output.perform_validations(
        perform_global_validations,
        Some(&StateDiff { storage_updates: expected_storage_updates, ..Default::default() }),
    );

    // Verify that the funded account, the new account and the sequencer all have changed balances.
    test_output.assert_account_balance_change(*FUNDED_ACCOUNT_ADDRESS);
    test_output.assert_account_balance_change(faulty_account_address);
    test_output.assert_account_balance_change(contract_address!(TEST_SEQUENCER_ADDRESS));

    // Validate poseidon usage.
    let poseidons = test_output
        .runner_output
        .metrics
        .execution_resources
        .builtin_instance_counter
        .get(&BuiltinName::poseidon)
        .unwrap();
    if use_kzg_da {
        expect![[r#"
            679
        "#]]
        .assert_debug_eq(poseidons);
    } else {
        expect![[r#"
            562
        "#]]
        .assert_debug_eq(poseidons);
    }
}
