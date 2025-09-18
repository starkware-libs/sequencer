use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::ACCOUNT_ID_0 as CAIRO1_ACCOUNT_ID;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::{
    test_resource_bounds_mapping,
    AccountTransactionGenerator,
    MultiAccountTransactionGenerator,
};
use mempool_test_utils::EMPTY_CONTRACT_CAIRO1_COMPILED_CLASS_HASH;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::{calculate_contract_address, CompiledClassHash};
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::transaction::TransactionVersion;
use starknet_api::{calldata, felt};
use starknet_types_core::felt::Felt;

use crate::common::{end_to_end_flow, validate_tx_count, TestScenario};

mod common;

const DEFAULT_TIP: u64 = 1_u64;
const CUSTOM_INVOKE_TX_COUNT: usize = 16;

/// Test a wide range of different kinds of invoke transactions.
#[tokio::test]
async fn custom_cairo1_txs() {
    end_to_end_flow(
        TestIdentifier::EndToEndFlowTestCustomSyscallInvokeTxs,
        create_custom_cairo1_txs_scenario(),
        GasAmount(110000000),
        true,
        false,
    )
    .await
}

fn create_custom_cairo1_txs_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: create_custom_cairo1_test_txs,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: |tx_hashes| validate_tx_count(tx_hashes, CUSTOM_INVOKE_TX_COUNT),
    }]
}

/// Creates a set of transactions that test the Cairo 1.0 syscall functionality.
/// The transactions are taken from starkware repo.
fn create_custom_cairo1_test_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let account_tx_generator = tx_generator.account_with_id_mut(CAIRO1_ACCOUNT_ID);
    let mut txs = vec![];
    txs.push(generate_empty_contract_declare_tx(account_tx_generator));
    txs.extend(generate_nested_library_call_invoke_txs(account_tx_generator));
    txs.extend(generate_direct_test_contract_invoke_txs(account_tx_generator));
    txs.extend(generate_test_deploy_txs(account_tx_generator, DEFAULT_TIP));
    txs.push(generate_test_get_execution_info_without_block_info_invoke_tx(account_tx_generator));

    txs
}

fn generate_direct_test_contract_invoke_txs(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let test_send_message_to_l1_args = vec![
        felt!(0_u64),    // target address
        felt!(2_u8),     // payload length
        felt!(4365_u64), // payload 1
        felt!(23_u64),   // payload 2
    ];
    let test_emit_events_args = vec![
        felt!(2_u64),    // number of arguments
        felt!(1_u64),    // key length
        felt!(2991_u64), // key
        felt!(2_u64),    // value length
        felt!(42_u64),   // value 1
        felt!(153_u64),  // value 2
    ];
    let test_keccak_args = vec![];
    let test_new_point_secp256k1_args = vec![
        felt!("0xE3E70682C2094CAC629F6FBED82C07CD"), // Low part of x (u256).
        felt!("0xF728B4FA42485E3A0A5D2F346BAA9455"), // High part of x (u256).
    ];
    let test_signature_verification_secp256k1_args = vec![]; // No arguments for this test.
    let test_new_point_secp256r1_args = vec![
        felt!("0x2D483FE223B12B91047D83258A958B0F"), // Low part of x (u256).
        felt!("0x502A43CE77C6F5C736A82F847FA95F8C"), // High part of x (u256).
    ];
    let test_signature_verification_secp256r1_args = vec![]; // No arguments for this test.
    let test_args = vec![felt!(3_u64), felt!(4_u64), felt!(5_u64)]; // Calldata has no meaning.

    [
        ("test_send_message_to_l1", test_send_message_to_l1_args),
        ("test_emit_events", test_emit_events_args),
        ("test_keccak", test_keccak_args),
        ("test_new_point_secp256k1", test_new_point_secp256k1_args),
        ("test_signature_verification_secp256k1", test_signature_verification_secp256k1_args),
        ("test_new_point_secp256r1", test_new_point_secp256r1_args),
        ("test_signature_verification_secp256r1", test_signature_verification_secp256r1_args),
        ("test", test_args),
    ]
    .iter()
    .map(|(fn_name, fn_args)| {
        let calldata = create_calldata(test_contract.get_instance_address(0), fn_name, fn_args);
        account_tx_generator
            .invoke_tx_builder()
            .tip(DEFAULT_TIP)
            .calldata(calldata)
            .build_rpc_invoke_tx()
    })
    .collect()
}

fn generate_nested_library_call_invoke_txs(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    // Define the arguments for the library calls.
    let test_sha256_args = vec![felt!(0_u64)]; // No arguments for test_sha256.
    let test_circuit_args = vec![felt!(0_u64)]; // No arguments for test_circuit.

    // Generate the invoke transactions for each library call.
    [("test_sha256", test_sha256_args), ("test_circuit", test_circuit_args)]
        .iter()
        .map(|(fn_name, fn_args)| {
            account_tx_generator.generate_library_call_invoke_tx(
                DEFAULT_TIP,
                &test_contract,
                &test_contract,
                fn_name,
                fn_args,
            )
        })
        .collect()
}

fn generate_empty_contract_declare_tx(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> RpcTransaction {
    // TODO(Itamar): Consider changing the empty contract to another contract with more functions
    // and check class.
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1(RunnableCairo1::Casm));
    // TODO(Itamar): Move compiled hash to the blockifier constants file as optional trait for
    // FeatureContract.
    let empty_compiled_class_hash =
        CompiledClassHash(felt!(EMPTY_CONTRACT_CAIRO1_COMPILED_CLASS_HASH));

    account_tx_generator
        .generate_rpc_declare_tx(empty_compiled_class_hash, empty_contract.get_sierra())
}

/// Deploy a contract and test the deployed contract functionality.
fn generate_test_deploy_txs(
    account_tx_generator: &mut AccountTransactionGenerator,
    tip: u64,
) -> Vec<RpcTransaction> {
    let mut txs = vec![];
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    // test_deploy_contract - constructor args and salt are unique to calculate contract address.
    let constructor_calldata_arg1 = felt!(1_u8);
    let constructor_calldata_arg2 = felt!(1_u8);
    let salt = felt!(7_u64);
    let test_deploy_args = vec![
        test_contract.get_class_hash().0, // class hash of the deployed contract.
        salt,                             // salt
        felt!(2_u64),                     // length of construct calldata.
        constructor_calldata_arg1,        // construct calldata: arg1.
        constructor_calldata_arg2,        // construct calldata: arg2.
        felt!(0_u64),                     // deploy_from_zero flag is down.
    ];
    let calldata =
        create_calldata(test_contract.get_instance_address(0), "test_deploy", &test_deploy_args);
    txs.push(
        account_tx_generator.invoke_tx_builder().tip(tip).calldata(calldata).build_rpc_invoke_tx(),
    );

    // Get the contract address of the newly deployed contract from test_deploy.
    let newly_deployed_contract_address = calculate_contract_address(
        ContractAddressSalt(salt),
        test_contract.get_class_hash(),
        // Constructor calldata of the deployed contract (test_contract).
        &calldata!(constructor_calldata_arg1, constructor_calldata_arg2),
        test_contract.get_instance_address(0), // deployer address
    )
    .expect("Failed to calculate contract address");

    // Write key and value to storage via test_call_contract of the deployed contract.
    let key = felt!(1948_u64);
    let test_storage_write_args = &[
        felt!(2_u64),    // arguments length
        key,             // key
        felt!(1967_u64), // value
    ];

    txs.push(account_tx_generator.generate_call_contract_invoke_tx(
        tip,
        &test_contract,
        &newly_deployed_contract_address,
        "test_storage_write",
        test_storage_write_args,
    ));

    // Read value by key from storage via test_library_call of the deployed contract.
    let test_storage_read_args = vec![
        felt!(1_u64), // arguments length
        key,
    ];
    txs.push(account_tx_generator.generate_library_call_invoke_tx(
        tip,
        &test_contract,
        &test_contract,
        "test_storage_read",
        &test_storage_read_args,
    ));

    // test_replace_class - replace the class of the deployed contract with an empty contract.
    let empty_contract = FeatureContract::Empty(account_tx_generator.account.cairo_version());
    let test_replace_class_args =
        vec![empty_contract.safe_get_sierra().unwrap().calculate_class_hash().0];
    let calldata = create_calldata(
        newly_deployed_contract_address,
        "test_replace_class",
        &test_replace_class_args,
    );

    txs.push(
        account_tx_generator.invoke_tx_builder().tip(tip).calldata(calldata).build_rpc_invoke_tx(),
    );

    txs
}

fn generate_test_get_execution_info_without_block_info_invoke_tx(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> RpcTransaction {
    let fn_name = "test_get_execution_info_without_block_info";
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    let resources_bounds_values = test_resource_bounds_mapping();
    let calldata = vec![
        TransactionVersion::THREE.0,                    // version
        *account_tx_generator.sender_address().0.key(), // account address
        Felt::ZERO,                                     // max fee
        Felt::from(3_u8),                               // length of resource bounds (Span)
        // Resource bounds values
        Felt::from_hex(&hex::encode("L1_GAS".as_bytes())).unwrap(),
        Felt::from(resources_bounds_values.l1_gas.max_amount.0),
        Felt::from(resources_bounds_values.l1_gas.max_price_per_unit.0),
        Felt::from_hex(&hex::encode("L2_GAS".as_bytes())).unwrap(),
        Felt::from(resources_bounds_values.l2_gas.max_amount.0),
        Felt::from(resources_bounds_values.l2_gas.max_price_per_unit.0),
        Felt::from_hex(&hex::encode("L1_DATA".as_bytes())).unwrap(),
        Felt::from(resources_bounds_values.l1_data_gas.max_amount.0),
        Felt::from(resources_bounds_values.l1_data_gas.max_price_per_unit.0),
        *account_tx_generator.sender_address().0.key(), // caller_address
        *test_contract.get_instance_address(0).0.key(), // contract_address
        selector_from_name(fn_name).0,
    ];
    let calldata = create_calldata(test_contract.get_instance_address(0), fn_name, &calldata);

    account_tx_generator.invoke_tx_builder().tip(0).calldata(calldata).build_rpc_invoke_tx()
}
