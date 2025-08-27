use std::collections::HashMap;
use std::sync::LazyLock;

use blockifier::state::cached_state::StateMaps;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::abi::abi_utils::get_storage_var_address;
use starknet_api::core::calculate_contract_address;
use starknet_api::executable_transaction::{DeclareTransaction, InvokeTransaction};
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::test_utils::{
    CHAIN_ID_FOR_TESTS,
    CURRENT_BLOCK_NUMBER,
    DEFAULT_STRK_L1_DATA_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    DEFAULT_STRK_L2_GAS_PRICE,
};
use starknet_api::transaction::constants::DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_api::{declare_tx_args, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::initial_state::create_default_initial_state_data;
use crate::test_manager::{TestManager, FUNDED_ACCOUNT_ADDRESS};
use crate::utils::{divide_vec_into_n_parts, get_class_info_of_cairo_1_feature_contract};

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
    let _initial_state = create_default_initial_state_data::<DictStateReader>().await;
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
    let (mut test_manager, mut nonce_manager) =
        TestManager::<DictStateReader>::new_with_default_initial_state().await;

    // Declare a test contract.
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let test_contract_sierra = test_contract.get_sierra();
    let class_hash = test_contract_sierra.calculate_class_hash();
    let compiled_class_hash = test_contract.get_real_compiled_class_hash();
    let declare_tx_args = declare_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        class_hash,
        compiled_class_hash,
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        nonce: nonce_manager.next(*FUNDED_ACCOUNT_ADDRESS),
    };
    let account_declare_tx = declare_tx(declare_tx_args);
    let class_info = get_class_info_of_cairo_1_feature_contract(test_contract);
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
    let invoke_tx_args = invoke_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        nonce: nonce_manager.next(*FUNDED_ACCOUNT_ADDRESS),
        calldata: deploy_contract_calldata,
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
    };
    let expected_contract_address = calculate_contract_address(
        contract_address_salt,
        class_hash,
        &Calldata(constructor_calldata[1..].to_vec().into()),
        *FUNDED_ACCOUNT_ADDRESS,
    )
    .unwrap();
    let deploy_contract_tx = invoke_tx(invoke_tx_args);
    let deploy_contract_tx =
        InvokeTransaction::create(deploy_contract_tx, &CHAIN_ID_FOR_TESTS).unwrap();
    test_manager.add_invoke_tx(deploy_contract_tx);
    test_manager.divide_transactions_into_n_blocks(n_blocks);
    let initial_block_number = CURRENT_BLOCK_NUMBER + 1;
    let test_output = test_manager
        .execute_test_with_default_block_contexts(initial_block_number, use_kzg_da, full_output)
        .await;

    let partial_state_diff = StateMaps {
        // Deployed contract.
        class_hashes: HashMap::from([(expected_contract_address, class_hash)]),
        // Storage update from the contract's constructor.
        storage: HashMap::from([(
            (expected_contract_address, get_storage_var_address("my_storage_var", &[])),
            arg1 + arg2,
        )]),
        // Declared class.
        compiled_class_hashes: HashMap::from([(class_hash, compiled_class_hash)]),
        ..Default::default()
    };

    let perform_global_validations = true;
    test_output.perform_validations(perform_global_validations, Some(&partial_state_diff));
}
