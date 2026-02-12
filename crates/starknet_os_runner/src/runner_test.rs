//! Integration tests for the Runner.

use std::sync::Arc;

use blockifier_test_utils::calldata::create_calldata;
use rstest::rstest;
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    PaymasterData,
    ProofFacts,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::InvokeTransactionV3;
use starknet_api::{calldata, felt, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::runner::VirtualSnosRunner;
use crate::test_utils::{
    default_resource_bounds_for_client_side_tx,
    fetch_sepolia_block_number,
    sepolia_runner_factory,
    DUMMY_ACCOUNT_ADDRESS,
    PRIVACY_POOL_CONTRACT_ADDRESS,
    PRIVACY_POOL_CONTRACT_NONCE,
    STRK_TOKEN_ADDRESS_SEPOLIA,
};

/// Integration test for the full Runner flow with a balance_of transaction.
///
/// Uses a dummy account on Sepolia that requires no signature validation.
/// This test verifies that the runner can successfully execute transactions and run the virtual OS
/// with state changes ( nounce ).
///
/// # Running
///
/// ```bash
/// SEPOLIA_NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_run_os_with_balance_of_transaction -- --ignored
/// ```
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access.
async fn test_run_os_with_balance_of_transaction() {
    // Creates an invoke transaction that calls `balanceOf` on the STRK token.
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS_SEPOLIA).unwrap();
    let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();

    // Calldata matches dummy account's __execute__(contract_address, selector, calldata).
    let calldata = create_calldata(strk_token, "balanceOf", &[account.into()]);
    let resource_bounds = default_resource_bounds_for_client_side_tx();

    let invoke_tx = invoke_tx(invoke_tx_args! {
        sender_address: account,
        calldata,
        resource_bounds,
    });

    // Create a custom factory with the specified run_committer setting.
    let factory = sepolia_runner_factory();
    let block_id = fetch_sepolia_block_number().await;

    // Verify execution succeeds.
    factory
        .run_virtual_os(block_id, vec![(invoke_tx)])
        .await
        .expect("run_virtual_os should succeed");
}

/// Integration test that runs a privacy pool transaction.
///
/// Uses a pre-signed privacy transaction that interacts with the privacy pool contract.
/// This test verifies that the runner can successfully execute privacy transactions.
///
/// # Running
///
/// ```bash
/// SEPOLIA_NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_run_os_with_privacy_transaction -- --ignored
/// ```
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access.
async fn test_run_os_with_privacy_transaction() {
    // Sender is the privacy pool contract.
    let sender_address = ContractAddress::try_from(PRIVACY_POOL_CONTRACT_ADDRESS).unwrap();

    // Signature for the specific transaction (any change in the tx changes the signature).
    let signature_r = felt!("0x20e2eb40a80ecb91fc20f8d67f5aeb597ca30a593785eddef26046352b639bd");
    let signature_s = felt!("0x6953e08cc5d88f01923afe940e009ad0d278319410fc52b0e050f379573b2a5");

    // Calldata semantics:
    // - Consumes note0 (60 STRK) and note1 (40 STRK).
    // - Creates:
    //   - note2: 90 STRK, randomness = 0xe08b0a271b4e1d1030f5f89ca0dbc8
    //   - note3: 10 STRK, randomness = 0xa167508bf91d497f245c6e1cf4e110
    let calldata = calldata![
        felt!("0x6ad5754abe954c193cee3d9b15ac84e4ac562dfac6287e2b99d56bb5e10adcb"),
        felt!("0x4"),
        felt!("0x5"),
        felt!("0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1"),
        felt!("0x3aab600ef074da54eaec6c828131ac970c62335d99f89da6dfe18eb55a7b648"),
        felt!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
        felt!("0x0"),
        felt!("0x5"),
        felt!("0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1"),
        felt!("0x3aab600ef074da54eaec6c828131ac970c62335d99f89da6dfe18eb55a7b648"),
        felt!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
        felt!("0x1"),
        felt!("0x3"),
        felt!("0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1"),
        felt!("0x6ad5754abe954c193cee3d9b15ac84e4ac562dfac6287e2b99d56bb5e10adcb"),
        felt!("0xfefe558519ee1cf0a1f6999eaa3d35d01ecb880badc6618fe26342fbee59aa"),
        felt!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
        felt!("0x4e1003b28d9280000"),
        felt!("0x2"),
        felt!("0xe08b0a271b4e1d1030f5f89ca0dbc8"),
        felt!("0x3"),
        felt!("0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1"),
        felt!("0x6ad5754abe954c193cee3d9b15ac84e4ac562dfac6287e2b99d56bb5e10adcb"),
        felt!("0xfefe558519ee1cf0a1f6999eaa3d35d01ecb880badc6618fe26342fbee59aa"),
        felt!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
        felt!("0x8ac7230489e80000"),
        felt!("0x3"),
        felt!("0xa167508bf91d497f245c6e1cf4e110")
    ];

    // If the nonce has changed, test_privacy_pool_contract_nonce_unchanged should fail.
    let nonce = Nonce(Felt::from_hex_unchecked(PRIVACY_POOL_CONTRACT_NONCE.data()));
    let tip = Tip(0);

    let tx = InvokeTransactionV3 {
        sender_address,
        signature: TransactionSignature(Arc::new(vec![signature_r, signature_s])),
        nonce,
        resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds {
            l1_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
            l2_gas: ResourceBounds {
                max_amount: GasAmount(10_000_000),
                max_price_per_unit: GasPrice(0),
            },
            l1_data_gas: ResourceBounds {
                max_amount: GasAmount(0),
                max_price_per_unit: GasPrice(0),
            },
        }),
        tip,
        calldata,
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: PaymasterData(vec![]),
        account_deployment_data: AccountDeploymentData(vec![]),
        proof_facts: ProofFacts::default(),
    };
    let invoke_tx = starknet_api::transaction::InvokeTransaction::V3(tx);

    let factory = sepolia_runner_factory();
    let block_id = fetch_sepolia_block_number().await;

    // Verify execution succeeds.
    factory
        .run_virtual_os(block_id, vec![(invoke_tx)])
        .await
        .expect("run_virtual_os should succeed");
}
